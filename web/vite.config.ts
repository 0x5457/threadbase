import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const apiTarget = process.env.THREADBASE_API ?? "http://127.0.0.1:7777";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "0.0.0.0",
    port: 5173,
    allowedHosts: ["omarchy.taile87f6.ts.net"],
    proxy: {
      "/api": apiTarget,
      "/ws": { target: apiTarget.replace(/^http/, "ws"), ws: true },
    },
  },
});
