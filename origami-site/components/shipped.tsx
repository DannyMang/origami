import { links } from "./links";

const shipped = [
  { title: "History", body: "Every save keeps a version you can compare and restore." },
  { title: "Export dialog", body: "Pick resolution, format, quality and range, and see the file size first." },
  { title: "Type tool", body: "Click the preview to add text right there, at the playhead." },
  { title: "Motion tab", body: "Keys and eases on a graph, tied to the lines in your script." },
  { title: "Voiceover", body: "Record a take while the timeline plays." },
  { title: "Captions", body: "Load an .srt file and style the subtitles in the script." },
];

export function Shipped() {
  return (
    <section className="section shipped bleed">
      <div className="shipped-head">
        <h2>Changelog</h2>
        <a href={links.changelog}>Read the changelog →</a>
      </div>
      <div className="cards">
        {shipped.map((item) => (
          <div key={item.title}>
            <h3>{item.title}</h3>
            <p>{item.body}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
