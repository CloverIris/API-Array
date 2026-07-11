import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "API ARRAY",
  description: "本地 API 资产、端点与发布工作区",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
