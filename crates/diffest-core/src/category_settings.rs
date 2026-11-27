use crate::DiffLayout;
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use tempfile::NamedTempFile;

const SETTINGS_LIMIT: u64 = 256 * 1024;
const SETTINGS_SIZE_ERROR: &str = "Settings exceed 256 KiB";
const SETTINGS_PATH: &str = ".diffest/settings.json";
const RULES_KEY: &str = "categoryRules";
const LAYOUT_KEY: &str = "defaultDiffLayout";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryRule {
    pub glob: String,
    pub category: String,
    pub subcategory: Option<String>,
}

#[derive(Serialize)]
pub struct FileCategory {
    pub category: String,
    pub subcategory: Option<String>,
}

#[derive(Serialize)]
pub struct RuleSettings {
    pub path: PathBuf,
    pub version: Option<String>,
    pub error: Option<String>,
    pub custom: bool,
    pub rules: Vec<CategoryRule>,
}

#[derive(Serialize)]
pub struct Categorization {
    #[serde(rename = "defaultDiffLayout")]
    pub default_diff_layout: DiffLayout,
    pub global: RuleSettings,
    pub project: Option<RuleSettings>,
    pub categories: Vec<FileCategory>,
}

type CompiledRules = Vec<(CategoryRule, GlobSet)>;

pub struct CategorySettings {
    global_path: PathBuf,
    last_valid: Mutex<SettingsCache>,
}

#[derive(Default)]
struct SettingsCache {
    rules: HashMap<PathBuf, CompiledRules>,
    default_layout: DiffLayout,
}

impl CategorySettings {
    pub fn new(home: &Path) -> Self {
        Self {
            global_path: home.join(SETTINGS_PATH),
            last_valid: Mutex::default(),
        }
    }

    pub fn read(&self, main_checkout: Option<&Path>, paths: &[String]) -> Categorization {
        let mut cache = self.last_valid.lock().unwrap();
        let SettingsCache {
            rules,
            default_layout,
        } = &mut *cache;
        let global = read_tier(rules, self.global_path.clone(), Some(default_layout));
        let project =
            main_checkout.map(|checkout| read_tier(rules, checkout.join(SETTINGS_PATH), None));
        let global_rules = rules.get(&global.path).unwrap_or(&BUILT_INS);
        let project_rules = project.as_ref().and_then(|tier| rules.get(&tier.path));
        let rules = project_rules.unwrap_or(global_rules);

        let categories = paths
            .iter()
            .map(|path| {
                let rule = rules.iter().find(|(_, pattern)| pattern.is_match(path));
                match rule {
                    Some((rule, _)) => FileCategory {
                        category: rule.category.clone(),
                        subcategory: rule.subcategory.clone(),
                    },
                    None => FileCategory {
                        category: "Other".into(),
                        subcategory: None,
                    },
                }
            })
            .collect();
        let global = RuleSettings {
            rules: global_rules.iter().map(|(rule, _)| rule.clone()).collect(),
            ..global
        };
        let project = project.map(|tier| RuleSettings {
            rules: rules.iter().map(|(rule, _)| rule.clone()).collect(),
            ..tier
        });
        Categorization {
            default_diff_layout: *default_layout,
            global,
            project,
            categories,
        }
    }

    pub fn save(
        &self,
        main_checkout: Option<&Path>,
        expected_version: &str,
        rules: Option<Vec<CategoryRule>>,
    ) -> Result<(), String> {
        let path = main_checkout
            .map(|checkout| checkout.join(SETTINGS_PATH))
            .unwrap_or_else(|| self.global_path.clone());
        let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
        let compiled = rules
            .map(compile)
            .transpose()
            .map_err(|error| describe(&error))?;
        let mut cache = self.last_valid.lock().unwrap();
        edit_settings(&path, expected_version, |json| {
            if let Some(rules) = &compiled {
                let rules = rules.iter().map(|(rule, _)| rule).collect::<Vec<_>>();
                json.insert(RULES_KEY.into(), serde_json::to_value(rules)?);
            } else {
                json.remove(RULES_KEY);
            }
            Ok(())
        })?;
        cache_rules(&mut cache.rules, &path, compiled);
        Ok(())
    }

    pub fn save_default_diff_layout(
        &self,
        expected_version: &str,
        layout: DiffLayout,
    ) -> Result<(), String> {
        let mut cache = self.last_valid.lock().unwrap();
        edit_settings(&self.global_path, expected_version, |json| {
            json.insert(LAYOUT_KEY.into(), serde_json::to_value(layout)?);
            Ok(())
        })?;
        cache.default_layout = layout;
        Ok(())
    }
}

fn edit_settings(
    path: &Path,
    expected_version: &str,
    edit: impl FnOnce(&mut Map<String, Value>) -> serde_json::Result<()>,
) -> Result<(), String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", path.display());
    let contents = read_file(path).map_err(|error| describe(&error))?;
    if version(&contents) != expected_version {
        return Err(describe(&"Settings changed on disk. Reload before saving."));
    }
    let mut json = read_object(contents.as_deref()).map_err(|error| describe(&error))?;
    edit(&mut json).map_err(|error| describe(&error))?;
    let directory = path.parent().expect("settings path has a parent directory");
    fs::create_dir_all(directory).map_err(|error| describe(&error))?;
    let mut file = NamedTempFile::new_in(directory).map_err(|error| describe(&error))?;
    serde_json::to_writer_pretty(&mut file, &json).map_err(|error| describe(&error))?;
    file.write_all(b"\n").map_err(|error| describe(&error))?;
    let size = file
        .as_file()
        .metadata()
        .map_err(|error| describe(&error))?
        .len();
    if size > SETTINGS_LIMIT {
        return Err(describe(&SETTINGS_SIZE_ERROR));
    }
    if read_file(path).map_err(|error| describe(&error))? != contents {
        return Err(describe(&"Settings changed on disk. Reload before saving."));
    }
    file.persist(path).map_err(|error| describe(&error))?;
    Ok(())
}

fn read_tier(
    cache: &mut HashMap<PathBuf, CompiledRules>,
    path: PathBuf,
    default_layout: Option<&mut DiffLayout>,
) -> RuleSettings {
    let contents = read_file(&path).map_err(|error| error.to_string());
    let version_on_disk = contents.as_ref().ok().map(version);
    let mut layout_error = None;
    let rules = contents.and_then(|contents| {
        let mut json = read_object(contents.as_deref()).map_err(|error| error.to_string())?;
        if let Some(default_layout) = default_layout {
            match json
                .remove(LAYOUT_KEY)
                .map(serde_json::from_value)
                .transpose()
            {
                Ok(layout) => *default_layout = layout.unwrap_or_default(),
                Err(error) => {
                    layout_error =
                        Some(format!("{}: Invalid {LAYOUT_KEY}: {error}", path.display()))
                }
            }
        }
        json.remove(RULES_KEY)
            .map(|value| {
                let rules = serde_json::from_value(value)
                    .map_err(|error| format!("Invalid {RULES_KEY}: {error}"))?;
                compile(rules)
            })
            .transpose()
    });
    let error = match rules {
        Ok(rules) => {
            cache_rules(cache, &path, rules);
            None
        }
        Err(error) => Some(format!("{}: {error}", path.display())),
    };
    RuleSettings {
        custom: cache.contains_key(&path),
        path,
        version: version_on_disk,
        error: error.or(layout_error),
        rules: Vec::new(),
    }
}

fn cache_rules(
    cache: &mut HashMap<PathBuf, CompiledRules>,
    path: &Path,
    rules: Option<CompiledRules>,
) {
    if let Some(rules) = rules {
        cache.insert(path.to_path_buf(), rules);
    } else {
        cache.remove(path);
    }
}

fn read_file(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut contents = Vec::new();
            file.take(SETTINGS_LIMIT + 1).read_to_end(&mut contents)?;
            if contents.len() as u64 > SETTINGS_LIMIT {
                return Err(std::io::Error::new(
                    ErrorKind::InvalidData,
                    SETTINGS_SIZE_ERROR,
                ));
            }
            Ok(Some(contents))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn version(contents: &Option<Vec<u8>>) -> String {
    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn read_object(contents: Option<&[u8]>) -> serde_json::Result<Map<String, Value>> {
    contents
        .map(serde_json::from_slice)
        .unwrap_or_else(|| Ok(Map::new()))
}

fn compile(rules: Vec<CategoryRule>) -> Result<CompiledRules, String> {
    if rules.len() > 256 {
        return Err("Settings support up to 256 category rules".into());
    }
    rules
        .into_iter()
        .enumerate()
        .map(|(index, rule)| {
            let invalid = |detail| format!("Rule {}: {detail}", index + 1);
            if rule.category.trim().is_empty() {
                return Err(invalid("Category must not be empty"));
            }
            if rule
                .subcategory
                .as_ref()
                .is_some_and(|name| name.trim().is_empty())
            {
                return Err(invalid("Subcategory must not be empty when provided"));
            }
            if rule.glob.trim().is_empty()
                || rule.glob.starts_with('/')
                || Path::new(&rule.glob).is_absolute()
                || rule.glob.split('/').any(|part| part == ".." || part == ".")
            {
                return Err(invalid("Glob must match a repository-relative path"));
            }
            if rule.glob.len() > 512 {
                return Err(invalid("Glob exceeds 512 bytes"));
            }
            let pattern = GlobBuilder::new(&rule.glob)
                .literal_separator(true)
                .backslash_escape(false)
                .build()
                .map_err(|error| format!("Rule {}: {error}", index + 1))?;
            let matcher = GlobSetBuilder::new()
                .add(pattern)
                .build()
                .map_err(|error| format!("Rule {}: {error}", index + 1))?;
            Ok((rule, matcher))
        })
        .collect()
}

static BUILT_INS: LazyLock<CompiledRules> = LazyLock::new(|| {
    let groups: &[(&str, &[&str])] = &[
        (
            "Generated",
            &[
                "**/{build,dist,target,coverage}/**",
                "**/*.generated.*",
                "**/*.min.js",
                "**/{Cargo.lock,pnpm-lock.yaml,yarn.lock,package-lock.json}",
            ],
        ),
        (
            "Tests",
            &[
                "**/{test,tests,__tests__}/**",
                "**/*{.test.,.spec.,_test.}*",
                "**/test_*",
            ],
        ),
        (
            "Docs",
            &[
                "**/{doc,docs}/**",
                "**/*.{md,rst}",
                "**/{README,CHANGELOG,LICENSE}*",
            ],
        ),
        (
            "Source",
            &[
                "**/{src,lib}/**",
                "**/*.{rs,js,jsx,ts,tsx,py,go,c,h,cpp,java,svelte,vue,css,html}",
            ],
        ),
    ];
    compile(
        groups
            .iter()
            .flat_map(|(category, patterns)| {
                patterns.iter().map(|glob| CategoryRule {
                    glob: (*glob).into(),
                    category: (*category).into(),
                    subcategory: None,
                })
            })
            .collect(),
    )
    .expect("built-in category rules are valid")
});
