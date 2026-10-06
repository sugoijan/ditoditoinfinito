export default function ddiInitializer() {
  if (window.__DDI_TRUNK_INIT) {
    return window.__DDI_TRUNK_INIT();
  }
  return {};
}
