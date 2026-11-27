export async function waitForDesktopDriver(until, readReady, coldWindowsStart) {
  if (!coldWindowsStart) {
    await until('owned application WebDriver server', readReady, 30_000);
    return;
  }

  // The listener can answer ready:false while the first WebView2 controller starts.
  const deadline = Date.now() + 60_000;
  await until(
    'owned application WebDriver listener',
    async () => typeof (await readReady()) === 'boolean',
    30_000,
  );
  await until('first WebView2 window', readReady, deadline - Date.now());
}
