/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'Noto Sans SC', 'sans-serif'],
      },
      colors: {
        accent: "#4F8CFF",
        surface: {
          DEFAULT: "var(--bg-primary)",
          secondary: "var(--bg-secondary)",
          elevated: "var(--bg-elevated)",
          card: "var(--bg-card)",
          input: "var(--bg-input)",
          tooltip: "var(--bg-tooltip)",
        },
        ink: {
          DEFAULT: "var(--text-primary)",
          secondary: "var(--text-secondary)",
          tooltip: "var(--text-tooltip)",
        },
        divider: "var(--border-color)",
        "hover-bg": "var(--hover-bg)",
        "selected-bg": "var(--selected-bg)",
        "accent-bg": "var(--accent-bg)",
        danger: "var(--error-color)",
      },
      keyframes: {
        "fade-in": {
          "0%": { opacity: "0" },
          "100%": { opacity: "1" },
        },
        "fade-out": {
          "0%": { opacity: "1" },
          "100%": { opacity: "0" },
        },
        "slide-up": {
          "0%": { opacity: "0", transform: "translateY(10px)" },
          "100%": { opacity: "1", transform: "translateY(0)" },
        },
      },
      animation: {
        "fade-in": "fade-in 0.4s ease-out forwards",
        "fade-in-fast": "fade-in 0.12s ease-out forwards",
        "fade-out": "fade-out 0.3s ease-in forwards",
        "slide-up": "slide-up 0.3s ease-out forwards",
      },
    },
  },
  plugins: [],
};
