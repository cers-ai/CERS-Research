import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@cers/contracts": fileURLToPath(
        new URL("../../packages/contracts/ts/domain.ts", import.meta.url),
      ),
      "@cers/ui/tokens.css": fileURLToPath(
        new URL("../../packages/ui/tokens.css", import.meta.url),
      ),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
