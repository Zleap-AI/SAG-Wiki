import { defineConfig } from 'fumadocs-mdx/config';
export default defineConfig({ mdxOptions: { remarkStructureOptions: { filterElement: () => false } } });
