import type { ReactNode } from "react";
import { Clip } from "./clip";

const K = ({ children }: { children: ReactNode }) => <span className="tok-k">{children}</span>;
const F = ({ children }: { children: ReactNode }) => <span className="tok-f">{children}</span>;
const S = ({ children }: { children: ReactNode }) => <span className="tok-s">{children}</span>;
const N = ({ children }: { children: ReactNode }) => <span className="tok-n">{children}</span>;

function Swap({ name, from, to }: { name: string; from: string; to: string }) {
  return (
    <span className={`swap swap-${name}`}>
      <span>{from}</span>
      <span>{to}</span>
    </span>
  );
}

const ruler = ["0:00", "0:02", "0:04", "0:06", "0:08", "0:10", "0:12", "0:14", "0:16"];

export function Demo() {
  return (
    <div className="demo" aria-hidden="true">
      <div className="demo-stage">
        <div className="editor">
          <div className="editor-title">
            <span className="editor-dots">
              <i />
              <i />
              <i />
            </span>
            my-video <span className="dim">/ main</span>
          </div>

          <div className="editor-left">
            <div className="editor-tab">
              <span>Program</span>
            </div>
            <div className="editor-preview">
              <Clip name="drift" className="editor-frame" />
            </div>
            <div className="editor-bar">
              Timeline
              <span className="editor-play" />
              <span className="dim">00:00:12:08 / 0:17.34</span>
            </div>
            <div className="editor-ruler">
              {ruler.map((t) => (
                <span key={t}>{t}</span>
              ))}
            </div>
            <div className="tracks">
              <div className="track">
                <span>V2</span>
                <div className="clip clip-text">&quot;first tracks&quot;</div>
              </div>
              <div className="track track-tall">
                <span>V1</span>
                <div className="clip clip-video clip-intro">intro</div>
                <div className="clip clip-video clip-funny">
                  funny 0:10–
                  <Swap name="trim" from="0:30" to="0:31" /> ×
                  <Swap name="speed" from="1.5" to="2.0" />
                </div>
              </div>
              <div className="track">
                <span>A1</span>
                <div className="clip clip-audio clip-intro">intro</div>
                <div className="clip clip-audio clip-funny">funny</div>
              </div>
              <div className="track">
                <span>A2</span>
                <div className="clip clip-audio clip-music">music vol 0.4 · fade 1/2</div>
              </div>
              <div className="playhead" />
            </div>
          </div>

          <div className="editor-right">
            <div className="editor-tab">
              edit.js <span className="dim">saved 11:29</span>
            </div>
            <pre className="code">
              <code>
                <span className="dim">{"// edit.js"}</span>
                {"\n"}
                <K>const</K> intro = <F>video</F>(<S>&quot;clips/intro.mp4&quot;</S>){"\n"}
                <K>const</K> funny = <F>video</F>(<S>&quot;clips/funny.mp4&quot;</S>){"\n"}
                <K>const</K> music = <F>sound</F>(<S>&quot;music/track.mp3&quot;</S>){"\n"}
                {"\n"}
                <K>const</K> body = <F>cat</F>({"\n"}
                {"  "}intro.<F>trim</F>(<S>&quot;0:00&quot;</S>, <S>&quot;0:04&quot;</S>),{"\n"}
                <span className="code-active">
                  {"  "}funny.<F>trim</F>(<S>&quot;0:10&quot;</S>,{" "}
                  <S>
                    &quot;
                    <Swap name="trim" from="0:30" to="0:31" />
                    &quot;
                  </S>
                  ).<F>speed</F>(
                  <N>
                    <Swap name="speed" from="1.5" to="2.0" />
                  </N>
                  ),
                </span>
                {"\n"}){"\n"}
                {"\n"}
                <F>stack</F>({"\n"}
                {"  "}body,{"\n"}
                {"  "}
                <F>text</F>(<S>&quot;first tracks&quot;</S>).<F>at</F>(<S>&quot;0:01&quot;</S>).<F>for</F>(<N>2</N>),{"\n"}
                {"  "}music.<F>volume</F>(<N>0.4</N>).<F>fade</F>(<N>1</N>, <N>2</N>),{"\n"})
              </code>
            </pre>
            <div className="slider">
              speed
              <span className="slider-track">
                <span className="slider-knob" />
              </span>
              <Swap name="speed" from="1.5" to="2.0" />
            </div>
          </div>

          <div className="editor-status">
            -- MOUSE -- <span className="dim">funny.trim(&quot;0:10&quot;, &quot;0:30&quot;).speed(1.5)</span>
          </div>

          <svg className="cursor" viewBox="0 0 26 30">
            <path d="M3 2 L3 24 L9 18.5 L13 27.5 L17 25.7 L13 17 L21 17 Z" />
          </svg>
        </div>
      </div>
    </div>
  );
}
