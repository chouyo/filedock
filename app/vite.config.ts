import { defineConfig, type PluginOption, type ViteDevServer } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";
import net from "node:net";
import { spawn } from "node:child_process";

const host = process.env.TAURI_DEV_HOST;

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), reactDevtools()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri expects a fixed port; if this is unavailable we fail fast
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching src-tauri
      ignored: ["**/src-tauri/**"],
    },
  },
  // Produce sourcemaps for debug builds
  build: {
    target: "es2021",
    minify: "esbuild",
    sourcemap: false,
  },
});

// React DevTools standalone bridge (dev only).
// Spawns the react-devtools window and injects the bridge script ahead of
// React, only inside the Tauri WebView. Uses a non-default port: other apps
// on the machine (e.g. Electron apps shipping the DevTools backend) connect
// to the default 8097 and would keep taking over the single connection.
const DEVTOOLS_PORT = Number(process.env.RDT_PORT) || 8098;

function reactDevtools(): PluginOption {
  const enabled = process.env.RDT !== "false";
  let child: ReturnType<typeof spawn> | null = null;

  const waitForPort = (port: number, timeoutMs = 15000) =>
    new Promise<boolean>((resolve) => {
      const start = Date.now();
      const probe = net.createConnection({ port });
      const done = (ok: boolean) => {
        probe.destroy();
        resolve(ok);
      };
      const retry = () => {
        if (Date.now() - start >= timeoutMs) return done(false);
        setTimeout(check, 300);
      };
      const check = () => {
        const c = net.createConnection({ port });
        c.once("connect", () => {
          c.destroy();
          done(true);
        });
        c.once("error", retry);
      };
      probe.once("connect", () => done(true));
      probe.once("error", retry);
    });

  return {
    name: "react-devtools",
    apply: "serve",
    configureServer(server: ViteDevServer) {
      if (!enabled) return;

      // Resolve the bin entry directly so we don't depend on PATH/.cmd lookup.
      const binPath = fileURLToPath(
        new URL("./node_modules/react-devtools/bin.js", import.meta.url),
      );

      // Spawn immediately; don't block on async checks before launching.
      child = spawn(process.execPath, [binPath], {
        env: { ...process.env, REACT_DEVTOOLS_PORT: String(DEVTOOLS_PORT) },
        stdio: "inherit",
        windowsHide: false,
      });
      child.on("error", (err) => {
        server.config.logger.error(
          `react-devtools failed to launch: ${err.message}`,
        );
      });

      // Best-effort readiness log (non-blocking).
      waitForPort(DEVTOOLS_PORT).then((ok) =>
        server.config.logger.info(
          ok
            ? `react-devtools ready on :${DEVTOOLS_PORT}`
            : `react-devtools not reachable on :${DEVTOOLS_PORT} (bridge inactive)`,
        ),
      );

      server.httpServer?.on("close", () => {
        try {
          child?.kill();
        } catch {
          /* ignore */
        }
      });
    },
    transformIndexHtml() {
      if (!enabled) return undefined;
      // The standalone DevTools accepts one connection at a time, so a plain
      // browser tab on the dev server would keep stealing it from the Tauri
      // WebView (and, lacking Tauri APIs, it renders no React roots, which
      // shows up as "Profiling not supported"). Only load the bridge inside
      // Tauri; document.write keeps it synchronous so it still runs before React.
      return [
        {
          tag: "script",
          children: `if ("__TAURI_INTERNALS__" in window) document.write('<script src="http://localhost:${DEVTOOLS_PORT}"><\\/script>');`,
          injectTo: "head-prepend",
        },
      ];
    },
  };
}
