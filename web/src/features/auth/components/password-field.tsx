import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/field";
import { t } from "@/lib/t";

/** A new-password field with the breached-password warning (frontend-spec 2.1): a warning, not a
 *  block; "Use it anyway" resubmits with `acceptBreachedPassword`. */
export function NewPasswordField({
  value,
  onChange,
  error,
  breached,
  onUseAnyway,
  label = t("auth.signup.password"),
}: {
  value: string;
  onChange: (v: string) => void;
  error?: string | undefined;
  breached: boolean;
  onUseAnyway: () => void;
  label?: string;
}) {
  return (
    <Field
      label={label}
      error={breached ? undefined : error}
      warning={
        breached ? (
          <span>
            {t("auth.breached.message")}{" "}
            <Button variant="link" className="text-sm text-warning-text underline" onClick={onUseAnyway}>
              {t("auth.breached.useAnyway")}
            </Button>
          </span>
        ) : undefined
      }
      hint={t("auth.signup.passwordHint")}
    >
      {(ids) => (
        <Input
          {...ids}
          type="password"
          autoComplete="new-password"
          required
          value={value}
          onChange={(e) => onChange(e.currentTarget.value)}
        />
      )}
    </Field>
  );
}
