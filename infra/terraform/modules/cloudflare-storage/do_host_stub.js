// Stub for the global DO host Worker (sessions / rate-limit DO classes).
// Real implementation is deployed via the Worker deploy pipeline.
// Terraform only manages the script declaration + bindings so that a
// drift-detection plan does not show a phantom script delete.
export default {
  async fetch() {
    return new Response("corelink-do-host: stub. See deploy pipeline.", { status: 503 });
  },
};
