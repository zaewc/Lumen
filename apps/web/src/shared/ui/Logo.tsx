/** Lumen's mark: the app icon, small (a 128 px rendering for sharp 2x and 3x screens). */
export function LogoMark({ className = "size-7" }: { readonly className?: string }) {
  return (
    <img
      src="/icon.png"
      alt=""
      aria-hidden="true"
      width={128}
      height={128}
      draggable={false}
      className={`inline-block shrink-0 select-none ${className}`}
    />
  );
}
