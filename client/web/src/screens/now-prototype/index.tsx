// THROWAWAY (#801, Phase 1). The prototype's one export, and the only line
// `FrontierBoard` knows about.
//
// Everything the board gains is `const proto = useNowFrontierPrototype()`,
// `prototype={proto}` on `FrontierColumns`, and `{proto?.bar}`. Deleting the
// prototype is deleting this directory and those three lines.

import { PrototypeBar } from "./PrototypeBar";
import { useNowPrototype, type FrontierPrototype } from "./seam";
import { variantA } from "./variant-a";
import { variantB } from "./variant-b";
import { variantC } from "./variant-c";
import { variantD } from "./variant-d";

export function useNowFrontierPrototype(): FrontierPrototype | undefined {
  return useNowPrototype(
    (host, variant) =>
      variant === "A"
        ? variantA(host)
        : variant === "B"
          ? variantB(host)
          : variant === "C"
            ? variantC(host)
            : variantD(host),
    (state) => <PrototypeBar key="proto-bar" {...state} />,
  );
}

export type { FrontierPrototype, PrototypeCtx } from "./seam";
