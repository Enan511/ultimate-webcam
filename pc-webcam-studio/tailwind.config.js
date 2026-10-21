/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./prompt.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        background: "#0c0d12",
        surface: {
          50: "#181a24",
          100: "#222533",
          200: "#2d3245",
          300: "#3d435c",
        },
        accent: {
          DEFAULT: "#00d26a",
          hover: "#00b85c",
          glow: "rgba(0, 210, 106, 0.25)"
        }
      },
      fontFamily: {
        sans: ["Segoe UI", "system-ui", "sans-serif"],
        mono: ["Consolas", "Courier New", "monospace"]
      }
    },
  },
  plugins: [],
}
