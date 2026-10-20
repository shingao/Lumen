import { useStore } from "../../app/store";
import { DetailPanel } from "../detail/DetailPanel";
import { Explorer } from "../explorer/Explorer";
import { KpiBand } from "../kpi/KpiBand";
import { Recommendations } from "../recommendations/Recommendations";
import styles from "./Screens.module.css";

export function AnalyseScreen() {
  const openReco = useStore((s) => s.openReco);
  return (
    <>
      <KpiBand />
      <div className={`${styles.panel} ${styles.split}`}>
        <Explorer />
        <aside className={styles.aside}>{openReco ? <DetailPanel key={openReco} id={openReco} /> : <Recommendations />}</aside>
      </div>
    </>
  );
}
