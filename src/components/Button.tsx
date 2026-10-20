import type { ButtonHTMLAttributes } from "react";
import styles from "./Button.module.css";

type Variant = "primary" | "clean" | "link" | "ghost";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant: Variant;
}

/** primary : noir (inversé en sombre) ; clean : vert « Nettoyer » ; link : texte souligné ; ghost : icône. */
export function Button({ variant, className, type = "button", ...rest }: Props) {
  return <button type={type} className={[styles.button, styles[variant], className].filter(Boolean).join(" ")} {...rest} />;
}
