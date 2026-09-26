export function showScreen(name: "lock" | "main"): void {
  document.getElementById("screen-lock")!.classList.toggle("hidden", name !== "lock");
  document.getElementById("screen-main")!.classList.toggle("hidden", name !== "main");
}
