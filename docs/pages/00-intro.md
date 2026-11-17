# Intro

hi and welcome to origami

my name is daniel. I made this because I started learning video editing in premiere + after effects recently and I'm a bit slow at it. I thought there were too many buttons, but felt like everything that is possible to do can be represented programmatically.

That might be super naive, considering I have not even spent a full month learning the nooks and crannies of it, but here are my thoughts so far on using such an interface:

- collapsing everything to code makes it much simpler to share what you did, as opposed to having someone guide you what buttons they pressed and what knobs they turned. Code makes the language of video editing sharable and reproducable.

- as AI models become smarter, I wanted to explore possible ways they can accelerate my workflow in video editing. Using agentic video editors seem to result in the same style videos, and using it to learn AE/Premiere seems inefficient still using today's tools that may utilize computer-use to walk you through which buttons to press. Porting every feature that exists in Premiere/AE into code creates this interface in which humans and agents can interact. 

- This is my personal experience with using my own platform so far:

  1. If you know how to do things within other programs, I don't think this approach would speed things up necessarily. I am able to color-grade an image to my liking much more faster, than if I were to try using an agent to do it for me.

     It could possibly play a role in the exploration phase. I tried colorgrading using an agent to do things for me, and had it create different possible looks for me to choose from (shoutout https://paper.design/)

     ![Contact sheets of film stock and cinematic looks tried on one clip](media/grade-explorations.png)

  2. If I do not know how to do something, it seems to be much easier and faster to do new things/effects I have never done before => if I want to try some ambitious effects, it seems it can be done much easier.

Anyways, like all my projects I hope this benefits somebody out there. Would love feedback on the platform as well! My X is https://x.com/DanielUng19

## Privacy

Origami works on your Mac and keeps your scripts, footage and exports there.

- **Update check.** Each time it opens, and when you pick Check for Updates…, Origami downloads one small file from GitHub to see if a newer version is out. It sends nothing about you or your project, but GitHub sees your IP address, as any website does. Start Origami with `ORIGAMI_NO_UPDATE_CHECK=1` set to turn the check off.
- **Usage and crash reports are off until you say yes.** The launch screen asks once: "Help improve Origami? Send anonymous usage and crash reports." Nothing is sent before you click Yes, and nothing is sent after you click No thanks. A copy of Origami built from source without a PostHog key never asks and never sends.
- **What is sent after a yes.** Reports go to PostHog (`us.i.posthog.com`), one small request each, for these events only:
  - `app opened`: each time Origami opens.
  - `exported`: when an export from the app ends, with the codec (`h264`, `hevc` or `prores`), the frame width and height, the length rounded to seconds, and whether it succeeded.
  - `type tool used`: when the T tool places a text.
  - `text edited on picture`: when you retype an existing text on the preview and save it.
  - `font picked`: when you choose from the font list. The font's name is not sent.
  - `voiceover recorded`: when a recorded take is added to the script.
  - `crash`: the next time Origami opens after a crash, with the file, line and column of the source code where it happened (such as `origami-app/src/view.rs:10:5`, or `unknown` when it does not look like one), and the error message only when it is one of a fixed list of standard messages made of set words and numbers, such as `index out of bounds: the len is 3 but the index is 7` or ``called `Option::unwrap()` on a `None` value``. Anything after the set words is dropped, and any other message is sent as just `panic`.

  Every event also carries a random install id, Origami's version, the macOS version and the chip type (`aarch64` or `x86_64`). The id is 16 random bytes made when you first say yes; it is not derived from your Mac, your name or your files. Events are marked so PostHog keeps no person profile for them and does not look up a location, but PostHog sees your IP address, as any website does.
- **What is never sent.** Script contents, the names of files, folders, media and fonts, paths, your user name, and anything from the `origami` command-line tool. A crash that happens while reports are off is not sent later.
- **Turning it off.** Pick Origami › Share Usage and Crash Reports in the menu bar to switch it off or back on; a tick marks it when it is on. Starting Origami with `ORIGAMI_NO_TELEMETRY=1` set sends nothing and hides both the question and the menu item. Your answer and the id are kept in `~/Library/Application Support/Origami/pulse`.
- **Nothing else leaves your Mac.** No account. A crash is always logged to `~/Library/Logs/Origami` on your Mac. Links to the docs and downloads open in your browser when you click them.
- **Saved versions.** Each save also keeps a copy of the script in `~/Library/Application Support/Origami/history`, readable only by your account, so History can show and restore it. It is never sent and never written into the project folder; see [History](editor.html#history) for how long copies are kept.
- **Microphone.** Used only while you record a voiceover.
- **Scripts.** A script can read any file your account can read when it names it, inside or outside its folder, as footage, sound, images, captions, fonts or LUTs, and what it reads can show in the preview and the export. It cannot use the network, write files other than its own `origami.lock`, or run other programs. Open scripts from people you trust, as you would any document that runs code.
