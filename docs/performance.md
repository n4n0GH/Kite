# Performance report, November 2026

Kite measured against Falkon and Firefox on 2026-11-16: startup, memory, idle CPU and Speedometer,
and memory in more detail with a second benchmark. The detailed benchmark found two things in Kite
that cost memory for every open tab. Both are changed in Kite 0.10.1, which is what the numbers
here are of unless they say 0.10.0. (It was measured as a build of 0.10.0 with these two changes,
before it was given its version.)

## Summary

- **Startup:** Kite opens a page 0.68 s after launch, Falkon after 0.65 s, Firefox after 1.15 s.
- **Idle CPU:** with five sites open, Kite uses 2.5 % of a core, Falkon 7.9 %, Firefox 8.2 %.
- **Memory:** Kite uses less than Firefox with any number of pages open (11 to 36 % less). Falkon
  uses less than Kite except on ad-heavy pages: 311 against 435 MiB with no page open, and 2028
  against 2259 MiB with 19 pages.
- **Memory per page:** on three of the four sets of pages an extra page costs Kite 2 to 7 MiB more
  than Falkon (65, 122 and 96 MiB), on ad-heavy pages 57 MiB less. In Kite 0.10.0 those three were
  82, 145 and 106 MiB.
- **Speedometer 3.1:** Kite scores 23.4 and 23.5 in two runs, Firefox 23.4 and 24.0, Falkon 26.7
  and 26.3.

## Setup

| | |
|---|---|
| Machine | AMD Ryzen 9 7900X3D (12 cores), 62 GiB RAM, AMD Radeon RX 7900 XTX |
| System | openSUSE Tumbleweed 20261111, Linux 7.3.3, KWin 6.8.2 (Wayland) |
| Kite | 0.10.0 (the released AppImage), and 0.10.1: 0.10.0 with tab pieces made on demand and a lighter scrollbar script (see [What was found and changed](#what-was-found-and-changed)), as an AppImage in the detailed benchmark and as its release binary in the general comparison |
| Falkon | 26.08.3, the distribution's package |
| Firefox | 158.0.1, the distribution's package |
| Engines | Kite and Falkon: QtWebEngine 6.11.2 (Chromium 140). Firefox: Gecko |

Every run started from a fresh profile with default settings and no extensions. The browsers ran
on a headless KWin with the machine's GPU, on a D-Bus session of their own, so the desktop and its
services took no part. Kite and Falkon block ads and trackers as they come (Kite's own blocker;
Falkon's AdBlock with EasyList), Firefox blocks trackers only.

## General comparison

| | Kite 0.10.1 | Falkon | Firefox |
|---|---:|---:|---:|
| Startup (median of 6) | 0.68 s | **0.65 s** | 1.15 s |
| Memory, 5 sites (median of 3) | 819 MiB | **768 MiB** | 1340 MiB |
| Idle CPU (median of 3) | **2.5 %** | 7.9 % | 8.2 % |
| Speedometer 3.1 (two runs) | 23.4 ± 1.4, 23.5 ± 1.2 | **26.7** ± 1.7, **26.3** ± 1.7 | 23.4 ± 1.4, 24.0 ± 0.87 |

Bold marks the best value in each row.

- **Startup** is the time from launching the browser to the `onload` of a local page. Kite's six
  starts took 0.67 to 0.68 s, Falkon's 0.64 to 0.67 s, Firefox's 1.13 to 1.17 s.
- **Memory** is the proportional set size (PSS) of the browser's whole process tree, 60 seconds
  after opening five sites at once: a Wikipedia article, a GitHub repository, an MDN page, BBC News
  and YouTube. The three rounds were 815 to 827 MiB for Kite, 762 to 774 MiB for Falkon and 1330
  to 1341 MiB for Firefox.
- **Idle CPU** is the processor time the same tree used over the following 30 seconds, as a share
  of one core.
- **Speedometer 3.1** is the benchmark at browserbench.org, with the margin it reports itself.

This is the comparison made for Kite 0.9.0 a week earlier (startup 0.72 s, memory 778 MB, idle CPU
3.7 %, Speedometer 24.5). The two can't be set side by side: that one ran on the live desktop, and
the pages of the memory test are live sites that differ from day to day. Falkon's memory came out
127 MiB lower this time with only a patch release in between, and both other browsers started about
8 % slower here.

## Memory in detail

The second benchmark follows the method of
[bnfy/blanc's memory benchmark](https://github.com/bnfy/blanc/tree/main/bench/memory), whose own
harness runs on macOS and Windows only:

- An idle baseline and four sets of pages, opened at once: 5 text pages, 9 mixed pages, 5 ad-heavy
  pages and 19 pages for scale.
- Three repetitions, with a new idle baseline in each and the order of the browsers rotated.
- Every run starts from a copy of a profile that was started once before, so no run includes a
  first start's one-time work.
- Memory is sampled every 5 seconds until three samples are within 2 % of each other, at least 20
  and at most 120 seconds.
- A run only counts if the browser's own history shows that it went to every page.

The measure is PSS plus swapped PSS over all the browser's processes, which counts memory shared
between processes once. It is not the measure blanc uses on macOS, so these numbers can't be set
beside blanc's. Memory held by the GPU is not included.

Two pages of blanc's list were changed. `tools.ietf.org/html/rfc2616` now redirects to another
domain and is replaced by the page it leads to. `www.forbes.com` is left out for all browsers: it
answers Kite with a captcha in place of its page, which Kite doesn't record as a visit, so no run of
Kite with it could count.

### Totals

Median of three repetitions, with the lowest and highest in brackets.

| | Kite 0.10.1 | Kite, blocker off | Falkon | Firefox |
|---|---:|---:|---:|---:|
| Idle | 435 MiB (434–435) | 435 MiB (434–437) | **311 MiB** (251–311) | 677 MiB (674–679) |
| Light, 5 pages | 760 MiB (756–764) | 742 MiB (737–744) | **626 MiB** (584–627) | 942 MiB (938–950) |
| Mixed, 9 pages | 1528 MiB (1526–1540) | 1754 MiB (1747–1781) | **1347 MiB** (1310–1379) | 1852 MiB (1789–1856) |
| Ad-heavy, 5 pages | **1454 MiB** (1442–1464) | 1543 MiB (1539–1767) | 1614 MiB (1506–1637) | 1626 MiB (1609–1759) |
| Scale, 19 pages | 2259 MiB (2209–2268) | 2690 MiB (2490–2700) | **2028 MiB** (2018–2042) | 2949 MiB (2924–3081) |

All 60 runs counted.

### Per page

What a further page costs: the loaded median less the idle median, divided by the number of pages.

| | Kite 0.10.1 | Kite, blocker off | Falkon | Firefox |
|---|---:|---:|---:|---:|
| Light | 65 MiB | 61 MiB | 63 MiB | **53 MiB** |
| Mixed | 122 MiB | 147 MiB | **115 MiB** | 131 MiB |
| Ad-heavy | 204 MiB | 222 MiB | 261 MiB | **190 MiB** |
| Scale | 96 MiB | 119 MiB | **90 MiB** | 120 MiB |

Kite's blocker saves 13 % on the mixed set and 16 % at 19 pages. On text pages there is nothing for
it to block, and it costs 18 MiB.

## What was found and changed

The first run of the detailed benchmark was with Kite 0.10.0 as released. Falkon, on the same
engine, used up to 31 % less: idle, and on three of the four sets of pages. Two causes were found
and removed.

**Interface pieces built for every tab.** Each tab came with its own page menu, tab menu, three
prompt bars, autofill popup, sign-in dialog, element picker and the pages shown for a crash or a bad
certificate, whether it ever showed them or not. With ten tabs of an empty page open, Kite's own
process grew by about 13 MiB of heap per tab, Falkon's by about 2 MiB. Measured one by one, with
eleven tabs open:

| Piece | Heap for eleven tabs | Per tab |
|---|---:|---:|
| Page context menu | 31 MiB | 2.8 MiB |
| The tab's own menu | 32 MiB | 2.9 MiB |
| Three prompt bars and the crash page | 33 MiB | 3.0 MiB |
| Autofill popup and sign-in dialog | 8 MiB | 0.7 MiB |
| Element picker | 3.6 MiB | 0.3 MiB |
| Certificate error page | 1.6 MiB | 0.15 MiB |

They are now made when a tab first needs them. The heap with eleven tabs went from 166 to 51 MiB
(both in a debug build).

**The overlay-scrollbar script.** Kite gives pages that draw their own scrollbars the overlay style
of all others. The script for it went through every element of every page, and again whenever the
page changed, which cost about 4 MiB per page (18 MiB over the five text pages). It now handles the
page's own scrollbar as the page loads, and any other when the pointer comes over it or it scrolls.
Until then such a scrollbar keeps the look its page gave it.

Kite 0.10.0 against 0.10.1, in runs two hours apart:

| | Kite 0.10.0 | Kite 0.10.1 | Falkon, first run → second |
|---|---:|---:|---:|
| Idle | 452 MiB | 435 MiB | 312 → 311 MiB |
| Light, 5 pages | 861 MiB | 760 MiB | 623 → 626 MiB |
| Mixed, 9 pages | 1761 MiB | 1528 MiB | 1334 → 1347 MiB |
| Ad-heavy, 5 pages | 1732 MiB | 1454 MiB | 1585 → 1614 MiB |
| Scale, 19 pages | 2457 MiB | 2259 MiB | 1964 → 2028 MiB |
| Per page: light / mixed / scale | 82 / 145 / 106 MiB | 65 / 122 / 96 MiB | 62 / 114 / 87 → 63 / 115 / 90 MiB |

Falkon's numbers stayed where they were, so the difference is Kite's. In the first run, one of
Kite's three mixed repetitions didn't count: stackoverflow.com was missing from its history.

Also measured, and left as they are:

- A site's own style sheet and script (Site Settings) cost nothing unless a site has them, and
  nothing measurable when ten sites do.
- The content blocker's page scripts save more than they cost: without them the five text pages
  took about 20 MiB more.
- With none of Kite's page scripts at all, the pages' processes were about 45 MiB lighter over five
  pages, more than the single scripts add up to. The likely reason is the fixed cost of running
  anything in every frame of a page; this was not verified.

## Open points

- **Idle memory.** With no page open Kite holds 435 MiB, Falkon 311 MiB. Part of it is the AppImage
  carrying its own Qt, which it shares with no other program: without file-backed memory the two
  are at 193 and 168 MiB. The rest was not looked into.
- **Speedometer.** Falkon is about 3 points ahead of Kite on the same engine.
- **forbes.com** shows Kite a captcha where Falkon and Firefox get the page. Kite's user agent and
  its privacy settings were ruled out as the cause.

## Limits

- One machine, one day, live sites fetched from Germany, with consent banners left unanswered.
- Empty profiles without extensions. Each browser at its default window size.
- Three repetitions per measurement. Where two ranges overlap, the browsers aren't told apart.
- The scripts that made these measurements are not in the repository yet.
