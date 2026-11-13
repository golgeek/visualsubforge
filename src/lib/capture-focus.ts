/** Capture the launcher now; only restore it if it still belongs to the DOM. */
export function captureFocus(): () => void {
  const target = document.activeElement;
  return () => {
    if (target instanceof HTMLElement && target.isConnected) target.focus();
  };
}
