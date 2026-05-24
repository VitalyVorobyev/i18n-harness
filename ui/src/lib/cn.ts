// Tiny class-name concatenation helper. No deps. Falsy parts are
// dropped so the call site can write conditional classes inline:
//
//   className={cn("base", active && "active", error && "error")}

export function cn(...parts: (string | false | null | undefined)[]): string {
  return parts.filter(Boolean).join(" ");
}
