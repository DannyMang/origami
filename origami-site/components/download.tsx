import { links, version } from "./links";
import { Logo } from "./logo";

export function Download() {
  return (
    <section className="download bleed">
      <Logo size={88} />
      <div className="download-name">
        origami
        <span>{version}</span>
      </div>
      <h2>Create beautiful videos easily.</h2>
      <a href={links.download} className="button button-primary button-large">
        Download Origami.dmg
      </a>
      <p>Free and open source · Apple silicon · macOS 15 or later</p>
    </section>
  );
}
