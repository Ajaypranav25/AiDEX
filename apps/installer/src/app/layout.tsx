import type { Metadata } from "next";
import "@aidex/ui/styles.css";

export const metadata: Metadata = {
  title: "AiDEX Setup",
  description: "Preview the AiDEX portable workspace setup flow. Nothing is written to a drive.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
