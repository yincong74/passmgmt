let timer: number | undefined;

export function toast(msg: string, ms = 3000): void {
  const el = document.getElementById("toast")!;
  el.textContent = msg;
  el.classList.remove("hidden");
  window.clearTimeout(timer);
  timer = window.setTimeout(() => el.classList.add("hidden"), ms);
}
