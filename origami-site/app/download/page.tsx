import type { Metadata } from "next";
import { links, version } from "@/components/links";
import { Logo } from "@/components/logo";

export const metadata: Metadata = { title: "Download Origami" };

export default function Download() {
  return (
    <main className="get">
      <iframe src={links.dmg} hidden />
      <div className="get-steps">
        <a href="/" className="get-brand">
          <Logo size={22} />
          origami
        </a>
        <div className="get-body">
          <h1>Open Origami in 3 steps</h1>
          <ol>
            <li>Open your Downloads folder</li>
            <li>Double-click Origami.dmg</li>
            <li>
              Drag Origami into Applications and open it
              <span>Signed and notarized by Apple: macOS will ask once before opening it.</span>
            </li>
          </ol>
          <p>
            Download didn&apos;t start? <a href={links.dmg}>Try again</a>
          </p>
        </div>
        <p>{version} · Apple silicon · macOS 15 or later</p>
      </div>
      <div className="get-art" aria-hidden="true">
        <div className="get-disk">
          <div className="get-disk-bar">
            <span className="editor-dots">
              <i />
              <i />
              <i />
            </span>
            Origami
          </div>
          <div className="get-disk-row">
            <div>
              <i>
                <Logo size={72} />
              </i>
              Origami
            </div>
            <svg viewBox="0 0 96 24">
              <path d="M2 12h88M80 4l10 8-10 8" />
            </svg>
            <div>
              <i>
                <svg width="56" height="48" viewBox="0 0 56 48" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinejoin="round">
                  <path d="M4 8a4 4 0 0 1 4-4h12l6 6h22a4 4 0 0 1 4 4v26a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4z" />
                </svg>
              </i>
              Applications
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}
