use glob::{MatchOptions, Pattern};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use tempfile::NamedTempFile;

const SETTINGS_PATH: &str = ".diffest/settings.json";
const RULES_KEY: &str = "categoryRules";

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
    pub global: RuleSettings,
    pub project: Option<RuleSettings>,
    pub categories: Vec<FileCategory>,
}

type CompiledRules = Vec<(CategoryRule, Pattern)>;

pub struct CategorySettings {
    global_path: PathBuf,
    last_valid: Mutex<HashMap<PathBuf, CompiledRules>>,
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
        let global = read_tier(&mut cache, self.global_path.clone());
        let project =
            main_checkout.map(|checkout| read_tier(&mut cache, checkout.join(SETTINGS_PATH)));
        let global_rules = cache.get(&global.path).unwrap_or(&BUILT_INS);
        let project_rules = project.as_ref().and_then(|tier| cache.get(&tier.path));
        let rules = project_rules.unwrap_or(global_rules);

        let categories = paths
            .iter()
            .map(|path| {
                let options = MatchOptions {
                    require_literal_separator: true,
                    ..MatchOptions::new()
                };
                let rule = rules
                    .iter()
                    .find(|(_, pattern)| pattern.matches_with(path, options));
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
        let contents = read_file(&path).map_err(|error| describe(&error))?;
        if version(&contents) != expected_version {
            return Err(describe(&"Settings changed on disk. Reload before saving."));
        }

        let mut json = read_object(contents.as_deref()).map_err(|error| describe(&error))?;
        if let Some(rules) = &compiled {
            let rules = rules.iter().map(|(rule, _)| rule).collect::<Vec<_>>();
            json.insert(
                RULES_KEY.into(),
                serde_json::to_value(rules).map_err(|error| describe(&error))?,
            );
        } else {
            json.remove(RULES_KEY);
        }
        let directory = path.parent().expect("settings path has a parent directory");
        fs::create_dir_all(directory).map_err(|error| describe(&error))?;
        let mut file = NamedTempFile::new_in(directory).map_err(|error| describe(&error))?;
        serde_json::to_writer_pretty(&mut file, &json).map_err(|error| describe(&error))?;
        file.write_all(b"\n").map_err(|error| describe(&error))?;
        if read_file(&path).map_err(|error| describe(&error))? != contents {
            return Err(describe(&"Settings changed on disk. Reload before saving."));
        }
        file.persist(&path).map_err(|error| describe(&error))?;
        cache_rules(&mut cache, &path, compiled);
        Ok(())
    }
}

fn read_tier(cache: &mut HashMap<PathBuf, CompiledRules>, path: PathBuf) -> RuleSettings {
    let contents = read_file(&path).map_err(|error| error.to_string());
    let version_on_disk = contents.as_ref().ok().map(version);
    let rules = contents.and_then(|contents| {
        let mut json = read_object(contents.as_deref()).map_err(|error| error.to_string())?;
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
        error,
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
    match fs::read(path) {
        Ok(contents) => Ok(Some(contents)),
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
            let pattern =
                Pattern::new(&rule.glob).map_err(|error| format!("Rule {}: {error}", index + 1))?;
            Ok((rule, pattern))
        })
        .collect()
}

static BUILT_INS: LazyLock<CompiledRules> = LazyLock::new(|| {
    let groups: &[(&str, &[&str])] = &[
        (
            "Generated",
            &[
                "**/build/**",
                "**/dist/**",
                "**/target/**",
                "**/coverage/**",
                "**/*.generated.*",
                "**/*.min.js",
                "**/Cargo.lock",
                "**/pnpm-lock.yaml",
                "**/yarn.lock",
                "**/package-lock.json",
            ],
        ),
        (
            "Tests",
            &[
                "**/test/**",
                "**/tests/**",
                "**/__tests__/**",
                "**/*.test.*",
                "**/*.spec.*",
                "**/*_test.*",
                "**/test_*",
            ],
        ),
        (
            "Docs",
            &[
                "**/doc/**",
                "**/docs/**",
                "**/*.md",
                "**/*.rst",
                "**/README*",
                "**/CHANGELOG*",
                "**/LICENSE*",
            ],
        ),
        (
            "Source",
            &[
                "**/src/**",
                "**/lib/**",
                "**/*.rs",
                "**/*.js",
                "**/*.jsx",
                "**/*.ts",
                "**/*.tsx",
                "**/*.py",
                "**/*.go",
                "**/*.c",
                "**/*.h",
                "**/*.cpp",
                "**/*.java",
                "**/*.svelte",
                "**/*.vue",
                "**/*.css",
                "**/*.html",
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
