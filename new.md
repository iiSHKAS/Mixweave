# Mixweave — Streamer Mode Design and Mute Treatment

## 1. Streamer Mode card composition

Every channel is presented as one outer card containing two equal-width vertical fader lanes. This applies to Master, Game, Chat, Media, Aux, and Mic.

The **Personal** lane is always on the left. The **Stream** lane is always on the right. Both remain inside the same channel card, sharing its channel name and application area.

The visual order inside each card is:

1. One centered channel header containing the channel icon and name.
2. Two adjacent fader lanes: Personal and Stream.
3. One shared monitoring-control row.
4. One application well spanning the card's inner width.

The application well belongs to the channel as a whole. It is not duplicated beneath the two individual outputs.

The colors in this description are the current dark-theme values.

### Outer card

| Property | Value |
|---|---|
| Corner radius | `16px` |
| Padding | `17px` top; `12px` left, right, and bottom |
| Border thickness | `1px` |
| Game / Chat / Media / Aux background | `linear-gradient(155deg, #353B43 0%, #2D333A 62%, #292E35 100%)` |
| Game / Chat / Media / Aux border | `#4B5561` |
| Master / Mic background | `linear-gradient(155deg, #2C3239, #272C32)` |
| Master / Mic border | `#46505B` |
| Channel name | `#EEF0F3` |
| Channel icon | `#BAC4CF` |

The channel header has a minimum height of `35px`. Its icon measures `21 × 21px`, with a `9px` gap before the name. The name is `16px`, weight `650`, with `-.35px` letter spacing.

The two-lane area begins `11px` below the header. It fills the card's inner width and uses a `6px` gap between its two lanes. The lanes divide the available width equally; their widths are not fixed pixel values.

The channel cards retain their shared row alignment. The Personal and Stream labels, volume readouts, fader stages, and lower control buttons align across adjacent cards.

## 2. Design of each Personal / Stream lane

Each output has its own rounded inner container. The separation is subtle in the normal state, but gives the mute effect an exact boundary when only one output is muted.

| Property | Value |
|---|---|
| Inner-container radius | `11px` |
| Border | `1px solid #55617065` |
| Background | `#FFFFFF04`, a very faint translucent white layer |
| Padding | `7px` top, `3px` left/right, `1px` bottom |
| Gap between the two containers | `6px` |

The contents of each lane appear in this order: output label, volume readout, vertical fader, and two small control buttons.

### Output label

The label is centered at the top of the lane. Personal uses a headphones icon; Stream uses a broadcast icon.

| Property | Value |
|---|---|
| Label-row minimum height | `25px` |
| Icon size | `14 × 14px` |
| Icon-to-text gap | `3px` |
| Font size | `12px` |
| Font weight | `650` |
| Label and icon color | `#C3CBD5` |

The visible words remain **Personal** and **Stream** in every mute state. Their positions do not change.

### Volume readout

The percentage sits below the output label and is centered on the fader.

| Property | Value |
|---|---|
| Readout area height | `43px` |
| Readout padding | `5px` top, `4px` bottom |
| Main number size | `23px` |
| Main number weight | `560` |
| Main number letter spacing | `-.75px` |
| Main number color | `#EEF0F3` |
| Percentage-symbol size | `12px` |
| Percentage-symbol weight | `500` |
| Percentage-symbol color | `#ADB7C5` |
| Number-to-symbol gap | `4px` |

Numbers use tabular figures, keeping their spacing stable as the displayed level changes.

### Vertical fader

The fader stage has a height of `clamp(220px, 31vh, 282px)`, with `4px` top margin and `6px` bottom margin. At viewport widths up to `580px`, the stage height is `245px`.

The interactive fader occupies the lane's available width, up to `82px`. Its rail is centered. Numeric scale labels are hidden in Streamer Mode; small ticks remain visible along the left side.

| Element | Geometry | Appearance |
|---|---|---|
| Rail | `9px` wide; `6px` radius | `#171C23` |
| Volume fill | Bottom-aligned inside the rail | `linear-gradient(to top, #EC6C3D, #FF8558)` |
| Small ticks | `3px` wide, `1px` high | `#727D8D` |
| Major ticks | `5px` wide, `1px` high | `#A8B3C2` |
| Tick-column position | `3px` from the fader's left edge | — |
| Handle | `32 × 26px`; `7px` radius | `linear-gradient(#65707D, #46515E)` |
| Handle border | `1px` | `#8793A3` |
| Normal handle grip | `18 × 2px` horizontal line | `#FFAB86` |

There are `21` ticks, with every fifth interval marked by a longer tick. Both lanes use the same visual scale and geometry.

### Lower controls

Each lane contains two buttons: a keyboard icon followed by a speaker/mute icon. The pair is centered beneath its fader.

| Property | Value |
|---|---|
| Button size | `28 × 29px` |
| Button radius | `7px` |
| Icon size | `16 × 16px` |
| Gap between buttons | `5px` |
| Control-row padding | `4px` top, `7px` bottom |
| Normal background | `#3B444F` |
| Normal border | `#5B6878` |
| Normal icon | `#C6D0DD` |
| Hover background | `#4E5B6A` |
| Hover border | `#8B9AAD` |
| Hover icon | `#F4F6F8` |

Master and Mic have one additional headphones button centered beneath the pair of lanes, outside both lane containers. This shared row is `39px` high, including `5px` bottom padding. Other channels reserve the same row height, preserving alignment of the application wells.

## 3. Visual scope of mute

The red treatment must match the exact muted output. A channel-wide effect appears only when both outputs are muted.

| Personal | Stream | Visual treatment |
|---|---|---|
| Unmuted | Unmuted | Normal dark card with two normal inner lanes |
| Muted | Unmuted | Red treatment inside the Personal lane only |
| Unmuted | Muted | Red treatment inside the Stream lane only |
| Muted | Muted | Entire outer card and application well receive the muted treatment; both lanes keep their individual indicators |

For a single muted output, the red boundary follows only that output's inner rounded container. It does not cross the `6px` gap into the other lane.

## 4. Muting one output: detailed appearance

The affected lane changes from a nearly transparent dark surface to a restrained burgundy gradient with a red border and a narrow red accent along its inside left edge.

| Element | Muted appearance |
|---|---|
| Lane background | `linear-gradient(155deg, #453036, #35282E)` |
| Lane border | `1px solid #D95D6C` |
| Inside left accent | `inset 2px 0 0 #EE6575` |
| Lane radius | Remains `11px` |
| Personal/Stream label | `#FFB5BD` |
| Label icon | Crossed-speaker icon in `#EE6575` |
| Main volume number | Remains `#EEF0F3` |
| Percentage symbol | `#CBB3BC` |
| All scale ticks | `#CBB3BC` |

The headphones or broadcast icon beside the affected output label becomes a crossed-speaker icon. The word Personal or Stream remains visible, so the user can immediately identify which output the red treatment belongs to.

The left accent is an inset shadow. It adds no width and causes no movement of the lane's contents.

The unmuted lane retains its normal surface, label, icon, handle, and controls. The outer channel card, channel header, shared monitoring row, and application well retain their ordinary visual state.

If that channel is selected, the normal orange selection border can remain around the outer card while the inner muted output has a red border. The two outlines communicate different states at different levels.

## 5. Diagonal hatch beside the muted fader

A narrow vertical hatch strip appears along the right side of the muted lane's fader stage. This gives the state a recognizable pattern in addition to its red color.

| Property | Streamer Mode value |
|---|---|
| Width | `4px` |
| Right inset | `1px` from the fader stage's right edge |
| Top inset | `16px` |
| Bottom inset | `16px` |
| Corner radius | `3px` |
| Hatch angle | `135deg` |
| Colored portion of each repeat | `3px` |
| Transparent portion of each repeat | `3px` |
| Hatch color | `#EE657575` |

The exact pattern is `repeating-linear-gradient(135deg, #EE657575 0 3px, transparent 3px 6px)`.

The final two digits of `#EE657575` specify alpha: the red marks have approximately `45.88%` opacity. The strip reveals the burgundy surface between its diagonal marks.

The strip remains separate from the central rail and the handle. It is decorative and does not block the fader. After appearing, the hatch is stationary; there is no continuous scrolling, flashing, or pulsing.

## 6. Muted handle and X mark

The muted fader's handle keeps its existing size and location, but gains a burgundy surface and a small red X.

| Property | Value |
|---|---|
| Handle size | `32 × 26px` |
| Radius | `7px` |
| Background | Solid `#54363F` |
| Border | `1px solid #D95D6C` |
| Inner highlight and shadow | `inset 0 1px 0 #FFFFFF15, 0 3px 6px #00000030` |
| X color | `#EE6575` |
| Each X stroke | `16px` long and `2px` thick |
| Stroke radius | `2px` |
| Stroke rotations | `-38deg` and `38deg` |

The X replaces the normal horizontal grip line and is centered inside the handle. It is not a separate badge beside the slider.

The burgundy handle and red X remain visible on hover. The handle does not return to its ordinary gray hover gradient while muted.

The handle stays at the displayed volume position. The volume number remains visible, and the orange rail fill retains its normal gradient. The red frame, hatch, handle mark, and mute button communicate mute independently of the fill.

## 7. Active mute-button appearance

The affected lane's speaker button becomes solid red and displays the crossed-speaker icon.

| Property | Value |
|---|---|
| Size | `28 × 29px` |
| Radius | `7px` |
| Background | `#EE6575` |
| Border | `1px solid #EE6575` |
| Icon | `#29131B`, sized `16 × 16px` |
| Shadow | `0 2px 6px #00000030` |
| Hover background and border | `#FF8290` |
| Hover icon | Remains `#29131B` |
| Hover movement | `translateY(-2px)` |
| Press movement | `translateY(0) scale(.94)` |

The keyboard button beside it retains its own ordinary or assigned-shortcut appearance. The other output's mute button remains neutral unless that output is also muted.

The existing lower mute button provides the action. No additional upper Unmute button, “Muted” box, or extra status panel is added.

## 8. Muting both outputs: whole-card appearance

When Personal and Stream are both muted, the outer channel card also changes to burgundy. This makes the full-channel state recognizable without removing the two individual output indicators.

### Outer card

| Element | Appearance |
|---|---|
| Background | `linear-gradient(155deg, #453036, #35282E)` |
| Border | `1px solid #D95D6C` |
| Radius | Remains `16px` |
| Inside left accent | `inset 3px 0 0 #EE6575` |
| Thin outer halo | `0 0 0 1px #EE657523` |
| Lower shadow | `0 5px 14px #00000030` |
| Channel-header icon | Original channel symbol, recolored `#FFB5BD` |
| Channel name | Remains `#EEF0F3` |

The outer card's accent is `3px` wide, slightly stronger than the `2px` accent used inside each muted lane.

If the fully muted card is selected, its red halo expands to `3px`: `0 0 0 3px #EE657523`. The ordinary orange selection border is replaced by the red muted-card border.

### Both inner lanes

Inside a fully muted card, the two lanes use a solid `#402E35` background and `#83505C` borders. Each retains its `2px` red left accent.

Both lanes continue to show:

- Their Personal or Stream label in `#FFB5BD`.
- A crossed-speaker label icon in `#EE6575`.
- A full-brightness volume number in `#EEF0F3`.
- Percentage and ticks in `#CBB3BC`.
- The red diagonal hatch strip.
- The burgundy handle with its red X.
- The active red mute button.

The shared monitoring button on Master or Mic keeps its own appearance. It is not automatically painted red as part of the full-channel treatment.

### Shared application well

| Element | Fully muted appearance |
|---|---|
| Well background | `#2C242B` |
| Well border | `#83505C` |
| Well radius | Remains `11px` |
| Applications heading | `#CBB3BC` |
| Empty-state text | `#CBB3BC` |
| Empty-state border | Dashed `#83505C` |

Program cards remain visually distinct within the tinted well: `#37414C` backgrounds, `#596777` borders, `#EEF0F3` names, and `#D0D8E2` icons. The mute effect does not fade or obscure them.

## 9. Returning from full mute to partial mute

When one output returns to its unmuted state, the outer card and shared application well return to their normal dark styling. The remaining muted lane keeps its red frame, burgundy surface, hatch, X-marked handle, and red mute button.

The restored lane returns to its own normal output icon: headphones for Personal or broadcast for Stream. Its handle returns to the normal gray gradient and horizontal grip line.

The lane positions, card dimensions, application-well position, and displayed volume values remain visually stable during this transition.

## 10. Animation of Streamer Mode and mute states

### Two-lane entrance

When the two-lane arrangement appears, the fader-bank area fades from opacity `0` to `1` and moves from `translateY(14px)` to its resting position. Duration: `300ms`. Easing: `cubic-bezier(.22, 1, .36, 1)`.

The entire outer card does not replay its staggered entrance animation during this mode change.

### Mute transition

| Visual part | Timing | Effect |
|---|---|---|
| Inner-lane border and left accent | `220ms`, default `ease` | Red framing appears or clears |
| Outer-card border and shadows, for full mute | `220ms`, default `ease` | Whole-card red framing appears or clears |
| Hatch opacity | `200ms`, default `ease` | Fades between `0` and `1` |
| Hatch scale | `250ms`, `cubic-bezier(.22, 1, .36, 1)` | Changes between `scaleY(.92)` and `scaleY(1)` |
| Mute-button background, border, and icon color | `200ms`, default `ease` | Changes between neutral and red |
| Mute-button hover/press transform | `200ms`, `cubic-bezier(.22, 1, .36, 1)` | Small lift or press |
| Handle background/shadow declarations | `200ms`, default `ease` | Applies the muted handle treatment |
| Shared application-well background and border | `200ms`, default `ease` | Changes only when the full card becomes muted or ceases to be fully muted |

The handle's X has no separate drawing or rotation animation; its two strokes appear with the mute state. Gradient changes are not implemented as a dedicated crossfade. The smooth visual feedback comes from the frame, shadows, solid-color transitions, and hatch opacity/scale.

With reduced motion enabled, these animation and transition durations become `.001ms`. The same red frame, hatch pattern, X mark, and button states remain visible.