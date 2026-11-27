import { Demo } from "./demo";
import { links } from "./links";

export function Hero() {
  return (
    <section className="hero bleed">
      <h1>A simple, programmatic video editor</h1>
      <div className="actions">
        <a href={links.download} className="button button-primary">
          Download for Mac
        </a>
        <a href={links.docs} className="button">
          Read the docs
        </a>
      </div>
      <Demo />
    </section>
  );
}
