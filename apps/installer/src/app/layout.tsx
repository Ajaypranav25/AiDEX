import type { Metadata } from "next";
import "@aidex/ui/styles.css";

export const metadata: Metadata = {
  title: "AiDEX Setup",
  description: "Set up a removable drive for the portable AiDEX workspace.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
