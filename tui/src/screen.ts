// A full-screen app needs a screen of its own.
//
// Rendering inline means the app starts wherever the previous output stopped,
// so an absolute mouse row does not match the row the layout computed, and any
// content taller than the window scrolls the terminal and leaves a scrollbar.
// The alternate buffer fixes both: the app starts at row 1 of a fresh screen
// that has no scrollback, and leaving restores whatever was there before.

const ENTER = "[?1049h[H[2J";
const LEAVE = "[?1049l";

let active = false;

export function enterAltScreen(write: (data: string) => void): () => void {
  if (active) return () => undefined;
  active = true;
  write(ENTER);

  const leave = (): void => {
    if (!active) return;
    active = false;
    write(LEAVE);
  };

  // Every exit path, including the ones that skip React's cleanup. Without
  // this the terminal is left showing an empty alternate buffer.
  process.once("exit", leave);
  process.once("SIGINT", leave);
  process.once("SIGTERM", leave);
  return leave;
}
