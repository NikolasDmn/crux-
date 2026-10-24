// Runs before first paint (index.html): the saved theme choice, so dark users never see a light flash.
try {
  var t = localStorage.getItem("crux.theme");
  if (t === "light" || t === "dark") document.documentElement.setAttribute("data-theme", t);
} catch (e) {
  /* storage blocked: "system" */
}
