import defaultMdxComponents from 'fumadocs-ui/mdx';
import { Cards, Card } from 'fumadocs-ui/components/card';
import { Callout } from 'fumadocs-ui/components/callout';
import { Steps, Step } from 'fumadocs-ui/components/steps';
import { ImageZoom } from 'fumadocs-ui/components/image-zoom';
import type { MDXComponents } from 'mdx/types';
import type { ComponentProps } from 'react';
import { RetrievalDiagram, EvidenceDiagram } from './sag2-diagrams';
function DocumentImage({src, ...props}: ComponentProps<'img'>) {
 const imageSource = typeof src === 'object' && 'src' in src && typeof src.src === 'string' ? src.src : src;
 const image = <img src={imageSource} {...props} />;
 return typeof imageSource === 'string' ? <ImageZoom src={imageSource} alt={props.alt}>{image}</ImageZoom> : image;
}
export function getMDXComponents(components?:MDXComponents) { return {...defaultMdxComponents, img: DocumentImage, Cards, Card, Callout, Steps, Step, RetrievalDiagram, EvidenceDiagram, ...components} satisfies MDXComponents; }
export const useMDXComponents = getMDXComponents;
declare global { type MDXProvidedComponents = ReturnType<typeof getMDXComponents>; }
