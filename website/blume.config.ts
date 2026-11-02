import { defineConfig } from "blume";

export default defineConfig({
  title: "GEDRoots",
  description:
    "Free, local genealogy software that keeps your family tree in a standard GEDCOM 7 file.",
  logo: { image: "/icon.png", text: "GEDRoots" },

  content: {
    root: "docs",
  },

  // Violet accent of the app (crates/gedroots/src/theme.rs), on neutral backgrounds.
  theme: {
    accent: { light: "#6D3DF0", dark: "#B997FF" },
    mode: "system",
  },

  markdown: {
    imageZoom: true,
  },

  navigation: {
    sidebar: { display: "group" },
  },

  github: {
    owner: "gedroots",
    repo: "gedroots",
    dir: "website",
  },

  // GitHub Pages serves the site under the repository name (set by the deploy workflow).
  deployment: {
    site: process.env.DOCS_SITE,
    base: process.env.DOCS_BASE,
  },

  footer: {
    copyright: "GEDRoots is free software. Your data stays on your computer.",
  },

});
