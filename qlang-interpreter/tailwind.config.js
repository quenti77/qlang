import daisyui from 'daisyui'

/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {},
  },
  darkMode: ['selector', '[data-theme="dark"]'],
  plugins: [daisyui],
  daisyui: {
    themes: false,
    darkTheme: "dark",
  },
}
