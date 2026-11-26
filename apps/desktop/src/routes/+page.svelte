<script lang="ts">
import icon from "../../src-tauri/icons/icon.png";
</script>

<main>
  <img src={icon} width="96" height="96" alt="" />
  <h1>Diffest</h1>
</main>

<style>
:global(html) {
  color-scheme: light dark;
  background: Canvas;
  color: CanvasText;
  font-family: system-ui, sans-serif;
}

:global(body) {
  margin: 0;
}

main {
  min-height: 100vh;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 1rem;
}

h1 {
  margin: 0;
  font-size: 2rem;
  letter-spacing: -0.035em;
}
</style>
