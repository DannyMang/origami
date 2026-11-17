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
- **Nothing else leaves your Mac.** No account, no analytics, no crash reports: a crash is logged to `~/Library/Logs/Origami` on your Mac only. Links to the docs and downloads open in your browser when you click them.
- **Microphone.** Used only while you record a voiceover.
- **Scripts.** A script can read the footage, sound, images, captions, fonts and LUTs it names. It cannot use the network, write files or run other programs.
