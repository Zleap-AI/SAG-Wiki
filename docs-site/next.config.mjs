import { createMDX } from 'fumadocs-mdx/next';
import { fileURLToPath } from 'node:url';

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
  reactStrictMode: true,
  async headers() { return [{source:'/docs-version.json',headers:[{key:'Cache-Control',value:'no-store'}]}]; },
  async redirects() {
    return [{ source: '/:lang(zh|en)/docs/data-import', destination: '/:lang/docs/data-sources', permanent: true }];
  },
  turbopack: { root: fileURLToPath(new URL('.', import.meta.url)) },
};

export default withMDX(config);
