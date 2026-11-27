"use client";

import { useEffect, useState } from "react";
import { Clip } from "./clip";

const tokens = [
  ["tok-k", "const"],
  ["", " ridge = "],
  ["tok-f", "video"],
  ["", "("],
  ["tok-s", '"ridge.mp4"'],
  ["", ")\n\nridge\n  ."],
  ["tok-f", "trim"],
  ["", "("],
  ["tok-n", "0"],
  ["", ", "],
  ["tok-n", "4"],
  ["", ")\n  ."],
  ["tok-f", "temperature"],
  ["", "("],
  ["tok-n", "-45"],
  ["", ")\n  ."],
  ["tok-f", "contrast"],
  ["", "("],
  ["tok-n", "40"],
  ["", ")\n  ."],
  ["tok-f", "saturation"],
  ["", "("],
  ["tok-n", "70"],
  ["", ")"],
];

const code = tokens.map(([, text]) => text).join("");
const kept = code.indexOf("\n");
const clips = ["hikers", "hikers-1", "hikers-2", "hikers-3"];
const grades = ["(-45)", "(40)", "(70)"].map((call) => code.indexOf(call) + call.length);

export function Language() {
  const [shown, setShown] = useState(code.length);

  useEffect(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let count = code.length;
    let typing = false;
    let timer = setTimeout(step, 2500);
    function step() {
      if (count === code.length) typing = false;
      if (count === kept) typing = true;
      count += typing ? 1 : -1;
      setShown(count);
      timer = setTimeout(step, count === code.length ? 2500 : count === kept ? 600 : typing ? 40 : 15);
    }
    return () => clearTimeout(timer);
  }, []);

  const grade = grades.filter((end) => shown >= end).length;
  let left = shown;

  return (
    <section className="section language bleed">
      <h2>A language that is designed to be minimal and easy.</h2>
      <div className="language-card" aria-hidden="true">
        <pre className="language-code">
          <code>
            {tokens.map(([name, text], index) => {
              const part = text.slice(0, left);
              left -= part.length;
              return (
                <span key={index} className={name}>
                  {part}
                </span>
              );
            })}
            <span>▍</span>
          </code>
        </pre>
        <div className="language-frame">
          {clips.map((name, index) => (
            <Clip key={name} name={name} className={index <= grade ? "on" : undefined} />
          ))}
        </div>
      </div>
    </section>
  );
}
