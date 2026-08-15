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

      const cloneHelper = `
const __totkMiiCloneDeep = (value, seen = new WeakMap()) => {
  if (value === null || typeof value !== "object") return value;
  if (seen.has(value)) return seen.get(value);
  if (value instanceof Date) return new Date(value.getTime());
  if (ArrayBuffer.isView(value)) return new value.constructor(value);
  if (value instanceof ArrayBuffer) return value.slice(0);
  const clone = Array.isArray(value) ? [] : Object.create(Object.getPrototypeOf(value));
  seen.set(value, clone);
  for (const key of Reflect.ownKeys(value)) clone[key] = __totkMiiCloneDeep(value[key], seen);
  return clone;
};
`;
      return {
        // MiiJS's prebuilt ESM chunk has incompatible CommonJS Lodash interop.
        // Its format tables contain functions, which must be retained by reference.
        code: cloneHelper + code.replace(/\b[A-Za-z_$][\w$]*(?:\.default)?\.cloneDeep\(/g, "__totkMiiCloneDeep("),
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
