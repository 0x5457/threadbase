import { useEffect } from "react";
import { installHaloTrigger } from "../lib/halo";

/** Mounts the click/focus halo; all rendering lives outside React. */
export function HaloLayer() {
  useEffect(() => installHaloTrigger(), []);
  return null;
}
