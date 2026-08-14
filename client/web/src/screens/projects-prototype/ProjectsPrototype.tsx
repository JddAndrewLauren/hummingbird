// PROTOTYPE (#449) — throwaway. Four variants of the Projects page, mounted
// on the existing Routes screen slot (App.tsx dev-gates this in; production
// still renders RoutesScreen), switchable via `?variant=` and the floating
// bar. The plan in one line: three structurally different layouts of the
// same fixture world — master–detail (a), gallery + dossier (b), outline
// (c) — plus the triage inline-create flow (t). World state is shared, so a
// flow started in one layout can be judged in another.
//
// To delete this prototype: this directory, App.tsx's dev branch, the two
// "Projects" strings in shell/screens.ts, and the prototype block at the
// bottom of components/core/Icon.tsx's ICON_MAP.

import { useState } from "react";
import { PrototypeBar } from "./PrototypeBar";
import { VariantA } from "./VariantA";
import { VariantB } from "./VariantB";
import { VariantC } from "./VariantC";
import { VariantT } from "./VariantT";
import { usePrototypeWorld } from "./world";

const VARIANTS = [
  { key: "a", name: "Master–detail" },
  { key: "b", name: "Gallery + dossier" },
  { key: "c", name: "Outline" },
  { key: "t", name: "Triage inline create" },
] as const;

type VariantKey = (typeof VARIANTS)[number]["key"];

function variantFromUrl(): VariantKey {
  const asked = new URLSearchParams(window.location.search).get("variant");
  const match = VARIANTS.find((variant) => variant.key === asked);
  return match ? match.key : "a";
}

export function ProjectsPrototype() {
  const [projects, api] = usePrototypeWorld();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [variant, setVariant] = useState<VariantKey>(variantFromUrl);

  function change(key: string) {
    setVariant(key as VariantKey);
    const url = new URL(window.location.href);
    url.searchParams.set("variant", key);
    window.history.replaceState(null, "", url);
  }

  const shared = { projects, api, selectedId, onSelect: setSelectedId };

  return (
    <>
      {variant === "a" ? <VariantA {...shared} /> : null}
      {variant === "b" ? <VariantB {...shared} /> : null}
      {variant === "c" ? <VariantC {...shared} /> : null}
      {variant === "t" ? <VariantT {...shared} /> : null}
      <PrototypeBar variants={VARIANTS} current={variant} onChange={change} />
    </>
  );
}
