import type { HTMLAttributes } from "react";
import {
  Activity,
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
  Feather,
  Flag,
  HelpCircle,
  Inbox,
  Info,
  Link,
  ListChecks,
  LoaderCircle,
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
  X,
  Zap,
} from "lucide-react";

// Lucide is the icon set (design README, ICONOGRAPHY) — but via the
// `lucide-react` package, not the design system's CDN `window.lucide`
// loader: the production CSP (`script-src 'self'`) allows no CDN scripts.
// The static import map keeps tree-shaking intact (a namespace import
// would bundle every glyph) and makes `name` a compile-checked union, so
// an icon missing from the map is a type error rather than a blank box.
export const ICON_MAP = {
  activity: Activity,
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
  feather: Feather,
  flag: Flag,
  "help-circle": HelpCircle,
  inbox: Inbox,
  info: Info,
  link: Link,
  "list-checks": ListChecks,
  "loader-circle": LoaderCircle,
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
  x: X,
  zap: Zap,
} as const;

export type IconName = keyof typeof ICON_MAP;

export interface IconProps extends HTMLAttributes<HTMLSpanElement> {
  /** Lucide icon name, kebab-case (e.g. "inbox", "bell"). */
  name: IconName;
  /** Rendered box in px. 16 inline, 18 default, 20 in toolbars, 24 on touch. */
  size?: number;
  /** Lucide stroke width. 1.75 is the Hummingbird default. */
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
