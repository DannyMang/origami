import type { Metadata } from "next";
import { Inter } from "next/font/google";
import localFont from "next/font/local";
import "./globals.css";

const inter = Inter({
  variable: "--font-inter",
  subsets: ["latin"],
  axes: ["opsz"],
});

const paperMono = localFont({
  src: "./fonts/PaperMono.woff2",
  variable: "--font-mono",
  weight: "100 800",
});

export const metadata: Metadata = {
  title: "Origami",
  description: "A simple, programmatic video editor.",
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={`${inter.variable} ${paperMono.variable}`}>
      <body>{children}</body>
    </html>
  );
}
