export function measurePaneControl(
  node: HTMLElement,
  measured: (height: number) => void,
) {
  let frame = 0;
  const observer = new ResizeObserver(() => {
    // These measurements resize an ancestor grid. Commit on the next frame
    // so layout does not feed back into the current observer delivery.
    if (!frame)
      frame = requestAnimationFrame(() => {
        frame = 0;
        measured(Math.ceil(node.getBoundingClientRect().height));
      });
  });
  observer.observe(node);
  return {
    destroy() {
      observer.disconnect();
      cancelAnimationFrame(frame);
    },
  };
}
