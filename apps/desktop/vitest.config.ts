import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    deps: {
      optimizer: {
        client: {
          enabled: true,
          include: ["@fluentui/react-components"],
        },
      },
    },
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test/setup.ts"],
  },
});
