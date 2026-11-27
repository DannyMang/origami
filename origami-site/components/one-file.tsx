const points = [
  {
    title: "Edit by typing or dragging",
    body: "Trim a clip on the timeline and the numbers in .trim() change with it.",
  },
  {
    title: "Share and reproduce your teammate's work easily",
    body: "Send the script instead of explaining which buttons you pressed.",
  },
  {
    title: "Stays on your Mac",
    body: "Scripts, footage and exports never leave your machine.",
  },
];

export function OneFile() {
  return (
    <section className="section bleed">
      <h2>Your entire video edit in one file.</h2>
      <div className="columns">
        {points.map((point) => (
          <div key={point.title}>
            <h3>{point.title}</h3>
            <p>{point.body}</p>
          </div>
        ))}
      </div>
    </section>
  );
}
