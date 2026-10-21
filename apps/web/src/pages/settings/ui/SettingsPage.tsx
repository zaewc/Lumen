import { APP_VERSION } from "@/shared/config";
import { LOCALE_NAMES, LOCALES, useI18n } from "@/shared/i18n";
import { isMac, userAgent } from "@/shared/lib";
import {
  GlobeIcon,
  Group,
  GroupRow,
  IconTile,
  InfoIcon,
  InfoTip,
  LogoMark,
  Page,
  Segmented,
  ShieldIcon,
} from "@/shared/ui";
import { AiReviewGroup } from "./AiReviewGroup";

/** English / 한국어, each named in its own language. */
function LanguageGroup() {
  const { locale, m, setLocale } = useI18n();
  return (
    <Group>
      <GroupRow
        icon={
          <IconTile tone="neutral">
            <GlobeIcon />
          </IconTile>
        }
        title={m.language}
        trailing={
          <Segmented
            label={m.language}
            value={locale}
            onChange={setLocale}
            segments={LOCALES.map((value) => ({ value, label: LOCALE_NAMES[value] }))}
          />
        }
      />
    </Group>
  );
}

/** Why some folders may be unread, and (on a Mac) how to allow Full Disk Access. */
function PermissionsGroup() {
  const { m } = useI18n();
  const mac = isMac(userAgent());
  const p = m.settings.permissions;
  return (
    <div id="permissions" className="scroll-mt-6">
      <Group
        title={p.title}
        note={
          <span className="flex items-start gap-1.5">
            <span className="flex-1">{p.unread}</span>
            <InfoTip term="coverage" />
          </span>
        }
      >
        <GroupRow
          icon={
            <IconTile tone="green">
              <ShieldIcon />
            </IconTile>
          }
          title={mac ? p.fda : p.access}
          description={
            mac ? (
              <>
                <p>{p.fdaIntro}</p>
                <p className="mt-2 font-medium text-ink">{p.steps}</p>
                <ol className="mt-1 list-decimal space-y-0.5 pl-5 marker:text-ink-3">
                  <li>{p.step1}</li>
                  <li>{p.step2}</li>
                  <li>{p.step3}</li>
                </ol>
              </>
            ) : (
              p.accessIntro
            )
          }
        />
      </Group>
    </div>
  );
}

/** The app's name and version, and where its data stays. */
function AboutGroup() {
  const { m } = useI18n();
  const a = m.settings.about;
  return (
    <Group title={a.title}>
      <GroupRow
        icon={<LogoMark className="size-7" />}
        title="Lumen"
        trailing={
          <span className="text-[13px] tabular-nums text-ink-2">
            {a.version} {APP_VERSION}
          </span>
        }
      />
      <GroupRow
        icon={
          <IconTile tone="neutral">
            <InfoIcon />
          </IconTile>
        }
        title={a.local}
        description={a.localBody}
      />
    </Group>
  );
}

/** Settings, grouped as in System Settings: AI review, language, permissions, about. */
export function SettingsPage() {
  const { m } = useI18n();
  return (
    <Page title={m.settings.title}>
      <div className="space-y-7">
        <AiReviewGroup />
        <LanguageGroup />
        <PermissionsGroup />
        <AboutGroup />
      </div>
    </Page>
  );
}
