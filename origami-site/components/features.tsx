import type { ReactNode } from "react";
import { Clip } from "./clip";

const wave =
  "M2 21v0M6 19v4M10 15v12M14 10v22M18 13v16M22 17v8M26 20v2M30 20v2M34 14v14M38 8v26M42 11v20M46 15v12M50 12v18M54 18v6M58 20v2M62 16v10M66 9v24M70 6v30M74 12v18M78 16v10M82 19v4M86 20v2M90 20v2M94 15v12M98 10v22M102 13v16M106 8v26M110 14v14M114 18v6M118 20v2M122 17v8M126 11v20M130 7v28M134 12v18M138 15v12M142 19v4M146 20v2M150 16v10M154 10v22M158 13v16M162 9v24M166 15v12M170 18v6M174 20v2M178 14v14M182 8v26M186 11v20M190 6v30M194 12v18M198 9v24M202 14v14M206 10v22";

function Mock({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="mock">
      <div className={`mock-stage mock-${name}`}>
        <img src={`/media/${name}.jpg`} alt="" />
        {children}
      </div>
    </div>
  );
}

const features = [
  {
    title: "Motion design made easy.",
    body: "Animate opacity, position and scale with keys and eases. Drag them in the Motion tab or the preview and the script follows.",
    shot: (
      <Mock name="motion">
        <div className="mock-frame">
          <Clip name="title" />
        </div>
        <i className="mock-playhead" />
      </Mock>
    ),
  },
  {
    title: "Voice over your videos.",
    body: "Hit record and talk while the timeline plays.",
    shot: (
      <Mock name="voiceover">
        <div className="mock-frame">
          <Clip name="falls" />
        </div>
        <div className="mock-take">
          <svg viewBox="0 0 211 42">
            <path d={wave} />
          </svg>
          <span>
            voice <b>take 1</b>
          </span>
        </div>
        <i className="mock-playhead" />
      </Mock>
    ),
  },
  {
    title: "Keep a revision history",
    body: "See what changed between versions and bring an earlier cut back with one click.",
    shot: (
      <div className="history">
        <img src="/media/history-list.png" alt="" />
        <img src="/media/history-diff.png" alt="" />
      </div>
    ),
  },
];

export function Features() {
  return (
    <section className="section bleed">
      <div className="section-head">
        <h2>A familiar, yet new, simple interface.</h2>
        <p>Everything you need to edit.</p>
      </div>
      <div className="features">
        {features.map((feature) => (
          <div key={feature.title} className="feature">
            <div className="feature-shot" aria-hidden="true">
              {feature.shot}
            </div>
            <div className="feature-text">
              <h3>{feature.title}</h3>
              <p>{feature.body}</p>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}
