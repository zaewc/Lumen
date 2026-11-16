/** A setup step's title and what it is about. */
export function StepHeading({
  id,
  title,
  intro,
}: {
  readonly id: string;
  readonly title: string;
  readonly intro: string;
}) {
  return (
    <>
      <h1 id={id} className="mt-5 text-title text-ink">
        {title}
      </h1>
      <p className="mt-2 text-ink-2">{intro}</p>
    </>
  );
}
