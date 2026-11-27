import { links, version } from "./links";
import { Logo } from "./logo";

const columns = [
  {
    label: "Product",
    items: [
      ["Download", links.download],
      ["Changelog", links.changelog],
      ["Install guide", links.install],
    ],
  },
  {
    label: "Learn",
    items: [
      ["Docs", links.docs],
      ["Language reference", links.language],
      ["Command line", links.cli],
    ],
  },
  {
    label: "Source",
    items: [
      ["GitHub", links.github],
      ["Build from source", links.github],
      ["License", links.license],
    ],
  },
];

export function Footer() {
  return (
    <footer className="footer bleed">
      <div className="footer-top">
        <div className="footer-brand">
          <a href="/">
            <Logo size={22} />
            origami
          </a>
          <p>A simple, programmatic video editor for the Mac.</p>
        </div>
        <div className="footer-columns">
          {columns.map((column) => (
            <div key={column.label}>
              <p className="label">{column.label}</p>
              {column.items.map(([name, href]) => (
                <a key={name} href={href}>
                  {name}
                </a>
              ))}
            </div>
          ))}
        </div>
      </div>
      <div className="footer-bottom">
        <p>Signed and notarized by Apple: macOS will ask once before opening it.</p>
        <p>
          {version} · GPL-3.0 · created by <a href={links.author}>Daniel Ung</a>
        </p>
      </div>
    </footer>
  );
}
