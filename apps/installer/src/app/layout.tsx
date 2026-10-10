import type { Metadata } from "next";
import "../styles.css";

export const metadata: Metadata = {
  title: "AiDEX Setup",
  description: "Preview the AiDEX portable workspace setup flow.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
