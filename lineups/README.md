# Lineups

Each lineup is a JSON file in a folder named for its map, like
`ascent/brimstone-a-main-to-default-f0cc.json`, with its clip beside it as an
`.mp4` of the same name and its pictures after that. The window's Lineups
screen writes them, and they're plain enough to copy to another PC or fix by
hand. Everything in here except this file stays out of git, since most clips
are cut from somebody else's video.

To give lineups to a friend, Share then Copy Code copies a code for the map's
lineups, and Copy Code under a lineup copies one for just that lineup. It holds
each lineup and its clip's link and times but not the clip, so a map's worth
comes to about 700 characters and fits in one chat message. Share then Paste
Code on their end checks every lineup in it, skips any they already have and
cuts each clip from its link, which takes a few seconds a clip. A clip cut from
a video on your own PC stays out of the code.

A link is downloaded into `.cache` while you trim it, and only the cut clip is
kept once you save. Anything left in `.cache` is deleted after a day, so it is
safe to empty any time.
