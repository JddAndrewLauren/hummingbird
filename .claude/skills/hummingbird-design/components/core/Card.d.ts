import * as React from "react";
export interface CardProps extends Omit<React.HTMLAttributes<HTMLElement>, "style"> {
  /** 0 flat · 1 resting (default) · 2 raised · 3 floating (dialogs, menus). */
  elevation?: 0 | 1 | 2 | 3;
  padding?: string;
  /** Adds hover lift + pointer; use for whole-card click targets. */
  interactive?: boolean;
  /** Brand-tinted border, for the one card that is the answer on screen. */
  accent?: boolean;
  as?: keyof JSX.IntrinsicElements;
  style?: React.CSSProperties;
  children?: React.ReactNode;
}
export declare function Card(props: CardProps): JSX.Element;
