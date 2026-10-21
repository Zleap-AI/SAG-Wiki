import { redirect } from 'next/navigation';
export default async function Page({params}:{params:Promise<{lang:string}>}) { redirect(`/${(await params).lang}/docs`); }
