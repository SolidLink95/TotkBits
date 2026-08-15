import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

function fixMiiJsLodashInterop() {
  return {
    name: "fix-miijs-lodash-interop",
    enforce: "pre",
    transform(code, id) {
      const normalizedId = id.replaceAll("\\", "/");
      if (!normalizedId.includes("/miijs/dist/chunks/") || !code.includes("cloneDeep(")) {
        return null;
      }

      return {
        // MiiJS's prebuilt ESM chunk has incompatible CommonJS Lodash interop.
        // These calls only clone plain mapping objects, so the platform clone is
        // equivalent and avoids both `default.cloneDeep` interop variants.
        code: code.replace(/\b[A-Za-z_$][\w$]*(?:\.default)?\.cloneDeep\(/g, "structuredClone("),
        map: null,
      };
    },
  };
}

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [fixMiiJsLodashInterop(), react()],
  optimizeDeps: {
    // Keep MiiJS visible to the transform above during the development server.
    exclude: ["miijs"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 3. tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
