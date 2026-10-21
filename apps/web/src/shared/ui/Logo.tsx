/** Lumen's mark: the app icon's indigo square and its spark of light, small. */
export function LogoMark({ className = "size-7" }: { readonly className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={`relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-[22%] bg-linear-to-br from-[#5b5bf7] via-[#4338ca] to-[#1e1b4b] shadow-sm shadow-indigo-950/30 ${className}`}
    >
      <svg aria-hidden="true" viewBox="0 0 32 32" className="size-full">
        <ellipse cx="16" cy="22" rx="9" ry="3.2" fill="#c7d2fe" opacity="0.95" />
        <ellipse cx="16" cy="19.4" rx="9" ry="3.2" fill="#ffffff" opacity="0.95" />
        <path
          d="M20.5 5.5c.5 3 1.4 3.9 4.4 4.4-3 .5-3.9 1.4-4.4 4.4-.5-3-1.4-3.9-4.4-4.4 3-.5 3.9-1.4 4.4-4.4Z"
          fill="#fffdf5"
        />
      </svg>
    </span>
  );
}
