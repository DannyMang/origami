import { Download } from "@/components/download";
import { Features } from "@/components/features";
import { Footer } from "@/components/footer";
import { Hero } from "@/components/hero";
import { Language } from "@/components/language";
import { Nav } from "@/components/nav";
import { OneFile } from "@/components/one-file";
import { Shipped } from "@/components/shipped";

export default function Home() {
  return (
    <>
      <Nav />
      <main>
        <Hero />
        <OneFile />
        <Features />
        <Language />
        <Shipped />
        <Download />
      </main>
      <Footer />
    </>
  );
}
