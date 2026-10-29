// Process-private OpenCode 1.19.3 TUI plugin. It observes navigation; it never submits input.
import { createRoot, createEffect } from "solid-js";
import { writeFile, rename } from "node:fs/promises";
const path = __NORMA_ROUTE_PATH__;
const nonce = __NORMA_ROUTE_NONCE__;
export default {
  id: "norma.current-session",
  tui(api) {
    let session = null, revision = 0, queued = null, writing = false, disposed = false;
    async function flush() {
      if (writing) return;
      writing = true;
      try {
        while (queued) {
          const value = queued;
          queued = null;
          await writeFile(path + ".pending", JSON.stringify(value), { mode: 0o600 });
          await rename(path + ".pending", path);
        }
      } catch {
        // An expired/missing heartbeat closes answer eligibility in Norma.
        queued = null;
      } finally { writing = false; }
    }
    function publish() {
      queued = { nonce, revision, session, updatedAt: Date.now() };
      void flush();
    }
    const timer = setInterval(() => { if (!disposed) publish(); }, 250);
    createRoot(dispose => {
      createEffect(() => {
        const route = api.route.current;
        const next = route.name === "session" ? route.params.sessionID : null;
        if (revision === 0 || next !== session) { session = next; revision++; publish(); }
      });
      api.lifecycle.onDispose(() => {
        disposed = true;
        clearInterval(timer);
        dispose();
        session = null;
        revision++;
        publish();
      });
    });
  },
};
