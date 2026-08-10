// Issue #108: "relation-blocked items … marked and the reason visible" —
// this turns the titles of a `BlockedFrontierEntryDTO`'s open blockers into
// that one line of reason text.

/** `titles` should be the titles of the blockers the store actually
 * returned (`BlockedFrontierEntryDTO.blockedBy`, already filtered to open
 * ones by `Core::blocked`) — this function does no filtering of its own. */
export function blockedReasonLabel(titles: readonly string[]): string {
  if (titles.length === 0) {
    return "Blocked";
  }
  if (titles.length === 1) {
    return `Blocked by: ${titles[0]}`;
  }
  const [last, ...rest] = [...titles].reverse();
  return `Blocked by: ${rest.reverse().join(", ")} and ${last}`;
}
