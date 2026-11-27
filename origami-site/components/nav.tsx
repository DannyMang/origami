import { links, version } from "./links";
import { Logo } from "./logo";

export function Nav() {
  return (
    <header className="bleed">
      <nav className="nav">
        <a href="/" className="nav-brand">
          <Logo size={22} />
          origami
        </a>
        <a href={links.docs}>Docs</a>
        <a href={links.changelog}>Changelog</a>
        <a href={links.github}>GitHub</a>
        <span className="nav-space" />
        <span className="nav-version">{version}</span>
        <a href={links.download} className="nav-download">
          Download
        </a>
      </nav>
    </header>
  );
}
