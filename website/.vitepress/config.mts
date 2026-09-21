import { defineConfig } from 'vitepress'
import { fileURLToPath } from 'node:url'
import footnote from 'markdown-it-footnote'
import { withMermaid } from 'vitepress-plugin-mermaid'

// `vitepress-plugin-mermaid` lists `mermaid` as a peer dependency and imports it from
// its own (isolated) install location, which does not see this project's `node_modules`.
// Alias `mermaid` to the local install so the bundled `Mermaid.vue` resolves it.
const mermaidEntry = fileURLToPath(
  new URL('../node_modules/mermaid/dist/mermaid.esm.min.mjs', import.meta.url),
)

// https://vitepress.dev/reference/site-config
export default withMermaid(defineConfig({
  title: "Malstrom",
  description: "Malstrom - Stateful, Distributed Stream Processing",
  themeConfig: {
    // https://vitepress.dev/reference/default-theme-config
    nav: [
      { text: 'Home', link: '/' },
    ],

    sidebar: [
      {
        text: 'About Malstrom',
        items: [
          { text: 'What is Malstrom?', link: '/WhatIsMalstrom' },
        ]
      },
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/guide/GettingStarted' },
          { text: 'Keyed Streams', link: '/guide/KeyedStreams' },
          { text: 'Stateful Programs', link: '/guide/StatefulPrograms' },
          { text: 'Joining and Splitting Streams', link: '/guide/JoiningSplitting' },
          { text: 'Timely Processing', link: '/guide/TimelyProcessing' },
          { text: 'Connecting to Kafka', link: '/guide/Kafka' },
          { text: 'Deploying to Kubernetes', link: '/guide/Kubernetes' },
          { text: 'Custom Sources', link: '/guide/CustomSources' },
          { text: 'Custom Sinks', link: '/guide/CustomSinks' },
          { text: 'Custom Operators', link: '/guide/CustomOperators' },
          { text: 'TTL Map Operator', link: '/guide/TtlMapOperator' },
        ]
      },
      {
        text: 'Internals',
        items: [
          { text: 'The Kvt Trait', link: '/internals/KvtTrait' },
          { text: 'The StartBuild Protocol', link: '/internals/StartBuild' },
        ]
      },
      {
        items: [
          { text: 'Malstrom compared to other frameworks', link: '/MalstromCompared' },
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/MalstromDevelopers/malstrom' }
    ]
  },
  markdown: {
    config: (md) => {
      md.use(footnote)
    }
  },
  // `vitepress-plugin-mermaid` renders ```mermaid blocks and follows VitePress's
  // dark mode (it switches the Mermaid theme when the site is dark). Keep the
  // diagrams to a syntax subset shared with the MkDocs `docs/` site (mermaid2).
  mermaid: {},
  vite: {
    resolve: {
      alias: { mermaid: mermaidEntry },
    },
  },
  sitemap: {
    hostname: 'https://malstrom.io'
  }
}))
