import type { HTMLAttributes } from "react";
import {
  Activity,
  ArrowLeft,
  ArrowUpRight,
  Bell,
  BellOff,
  Calendar,
  CalendarClock,
  Check,
  ChevronDown,
  CircleCheck,
  Clock,
  CloudFog,
  Database,
  Download,
  Ellipsis,
  Feather,
  FolderKanban,
  Flag,
  HelpCircle,
  Inbox,
  Info,
  Link,
  ListChecks,
  LoaderCircle,
  Mic,
  Moon,
  Play,
  Plus,
  Radio,
  RefreshCw,
  RotateCcw,
  Route,
  ScrollText,
  Search,
  Settings,
  Siren,
  Sparkles,
  Sun,
  Trash2,
  X,
  Zap,
} from "lucide-react";
import {
  EnergyHigh,
  EnergyLow,
  EnergyMedium,
  EnergyUnset,
  SizeDeep,
  SizeNormal,
  SizeQuick,
  SizeUnset,
} from "./custom-glyphs";

// Lucide is the icon set (design README, ICONOGRAPHY) — but via the
// `lucide-react` package, not the design system's CDN `window.lucide`
// loader: the production CSP (`script-src 'self'`) allows no CDN scripts.
// The static import map keeps tree-shaking intact (a namespace import
// would bundle every glyph) and makes `name` a compile-checked union, so
// an icon missing from the map is a type error rather than a blank box.
export const ICON_MAP = {
  activity: Activity,
  // #624: the Projects dossier's back affordance. `chevron-down` is the
  // disclosure glyph and says nothing about going back; this is a
  // navigation move, so it gets the navigation arrow.
  "arrow-left": ArrowLeft,
  "arrow-up-right": ArrowUpRight,
  bell: Bell,
  "bell-off": BellOff,
  calendar: Calendar,
  "calendar-clock": CalendarClock,
  check: Check,
  "chevron-down": ChevronDown,
  "circle-check": CircleCheck,
  clock: Clock,
  "cloud-fog": CloudFog,
  database: Database,
  download: Download,
  ellipsis: Ellipsis,
  feather: Feather,
  // #624: the Projects screen's nav glyph, replacing `route`'s. A project
  // is a board of actions with a Route on it, not the Route itself.
  "folder-kanban": FolderKanban,
  flag: Flag,
  "help-circle": HelpCircle,
  inbox: Inbox,
  info: Info,
  link: Link,
  "list-checks": ListChecks,
  "loader-circle": LoaderCircle,
  // The capture box's dictation control (#379). One glyph for both states:
  // listening is `IconButton`'s `active` (the design system's own toggled-on
  // treatment — ember tint plus brand foreground), not a second icon.
  // `mic-off` would say "muted", which is not what stopping means.
  mic: Mic,
  moon: Moon,
  play: Play,
  plus: Plus,
  radio: Radio,
  "refresh-cw": RefreshCw,
  "rotate-ccw": RotateCcw,
  route: Route,
  "scroll-text": ScrollText,
  search: Search,
  settings: Settings,
  siren: Siren,
  sparkles: Sparkles,
  sun: Sun,
  "trash-2": Trash2,
  x: X,
  zap: Zap,
  // The two families Lucide has no glyph for, drawn in `custom-glyphs.tsx`
  // (#446, ADR-0024). They are ordinary entries here because they take the
  // same props Lucide's do — the level is in the glyph, the colour comes
  // from `screens/size-energy.ts`, and `Icon` itself learns nothing.
  "size-unset": SizeUnset,
  "size-quick": SizeQuick,
  "size-normal": SizeNormal,
  "size-deep": SizeDeep,
  "energy-unset": EnergyUnset,
  "energy-low": EnergyLow,
  "energy-medium": EnergyMedium,
  "energy-high": EnergyHigh,
} as const;

export type IconName = keyof typeof ICON_MAP;

export interface IconProps extends HTMLAttributes<HTMLSpanElement> {
  /** A key of `ICON_MAP`, kebab-case (e.g. "inbox", "bell") — a Lucide glyph,
   * or one of the eight size/energy glyphs `custom-glyphs.tsx` draws. */
  name: IconName;
  /** Rendered box in px. 16 inline, 18 default, 20 in toolbars, 24 on touch. */
  size?: number;
  /** Stroke width for the Lucide glyphs; 1.75 is the Hummingbird default.
   * The size/energy glyphs accept it and ignore it — their ramp is drawn at
   * a fixed weight, and `custom-glyphs.tsx`'s header says why. */
  strokeWidth?: number;
  color?: string;
  /** Supply only for a standalone, meaningful icon; otherwise it stays aria-hidden. */
  title?: string;
}

export function Icon({
  name,
  size = 18,
  strokeWidth = 1.75,
  color = "currentColor",
  title,
  style = {},
  ...rest
}: IconProps) {
  const Glyph = ICON_MAP[name];
  return (
    <span
      aria-hidden={title ? undefined : true}
      aria-label={title}
      role={title ? "img" : undefined}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: size,
        height: size,
        color,
        flex: "0 0 auto",
        ...style,
      }}
      {...rest}
    >
      <Glyph size={size} strokeWidth={strokeWidth} aria-hidden />
    </span>
  );
}
