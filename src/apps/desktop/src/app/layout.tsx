import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "API ARRAY",
  description: "本地 API 资产、审计端点与编组工作区",
  icons: { icon: "/icon.svg" },
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return <html lang="zh-CN"><body>{children}</body></html>;
}
