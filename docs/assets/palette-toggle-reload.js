// Mermaid diagrams are rendered on page load using the active color scheme
// (see mermaid2 `arguments.theme` in mkdocs.yml). Mermaid does not observe
// MaterialX palette toggles on its own, so reload the page when the user
// switches light/dark mode; MaterialX persists the selected palette in
// localStorage, so the reload keeps the user's choice.
document.addEventListener("DOMContentLoaded", function () {
  document.querySelectorAll('input[name="__palette"]').forEach(function (input) {
    input.addEventListener("change", function () {
      location.reload();
    });
  });
});