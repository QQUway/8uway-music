import { defineConfig } from "astro/config";

export default defineConfig({
  output: "static",
  server: {
    proxy: {
      "/api": "http://127.0.0.1:8080",
      "/audio": "http://127.0.0.1:8080",
    },
  },
});
