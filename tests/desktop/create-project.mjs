// The first Save Project uses a web form. Subsequent saves use the attachment.
export async function installCreateProject(browser) {
  await browser.execute(() => {
    window.__submitCreateProject = async (path) => {
      const end = performance.now() + 10000;
      let form;
      while (
        !(form = document.querySelector(
          'dialog[open][aria-labelledby="create-project-title"] form',
        ))
      ) {
        if (performance.now() > end)
          throw new Error('Create Project dialog did not open');
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      const split = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\')) + 1;
      for (const [name, value] of [
        ['folder', path.slice(0, split)],
        ['projectName', path.slice(split)],
      ]) {
        const input = form.elements.namedItem(name);
        input.value = value;
        input.dispatchEvent(new Event('input', { bubbles: true }));
      }
      form.requestSubmit();
    };
  });
}
export async function submitCreateProject(browser, path) {
  await installCreateProject(browser);
  await browser.execute((path) => window.__submitCreateProject(path), path);
}
