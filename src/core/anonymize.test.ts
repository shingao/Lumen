import { describe, expect, it } from "vitest";
import { anonymizeName, anonymizePath, MASK } from "./anonymize";

const ctx = { profile: "C:\\Users\\Arthur" };

describe("anonymizePath", () => {
  it("remplace le profil par %USERPROFILE%", () => {
    expect(anonymizePath("C:\\Users\\Arthur", ctx)).toBe("%USERPROFILE%");
    expect(anonymizePath("c:\\users\\arthur\\AppData\\Local\\npm-cache", ctx)).toBe("%USERPROFILE%\\AppData\\Local\\npm-cache");
    expect(anonymizePath("~\\AppData\\Local\\Temp", ctx)).toBe("%USERPROFILE%\\AppData\\Local\\Temp");
    expect(anonymizePath("~", ctx)).toBe("%USERPROFILE%");
    expect(anonymizePath("~\\.cache\\pip, ~\\AppData\\Local\\uv", ctx)).toBe("%USERPROFILE%\\.cache\\pip, %USERPROFILE%\\AppData\\Local\\uv");
    // Un autre dossier qui commence pareil n'est pas le profil.
    expect(anonymizePath("C:\\Users\\Arthur2\\x", ctx)).toBe(`C:\\Users\\${MASK}\\x`);
  });

  it("masque les noms sous les dossiers personnels", () => {
    expect(anonymizePath("C:\\Users\\Arthur\\Documents\\Impôts 2025\\déclaration.pdf", { ...ctx, file: true })).toBe(
      `%USERPROFILE%\\Documents\\${MASK}\\${MASK}.pdf`,
    );
    expect(anonymizePath("~\\Videos\\Vacances Corse", ctx)).toBe(`%USERPROFILE%\\Videos\\${MASK}`);
    expect(anonymizePath("~\\OneDrive - Contoso\\Contrat.docx", { ...ctx, file: true })).toBe(`%USERPROFILE%\\OneDrive - Contoso\\${MASK}.docx`);
    expect(anonymizePath("~\\Desktop", ctx)).toBe("%USERPROFILE%\\Desktop");
    expect(anonymizePath("~\\Documents\\Banque, relevés.pdf", { ...ctx, file: true })).toBe(`%USERPROFILE%\\Documents\\${MASK}.pdf`);
    // Une extension douteuse n'est pas gardée.
    expect(anonymizePath("~\\Downloads\\nom.très-long-secret", { ...ctx, file: true })).toBe(`%USERPROFILE%\\Downloads\\${MASK}`);
    // Hors dossiers personnels : rien n'est masqué.
    expect(anonymizePath("~\\AppData\\Local\\Discord\\Cache", ctx)).toBe("%USERPROFILE%\\AppData\\Local\\Discord\\Cache");
  });

  it("masque le nom des autres comptes, pas les profils partagés", () => {
    expect(anonymizePath("D:\\Users\\Bob\\Pictures\\photo.jpg", { ...ctx, file: true })).toBe(`D:\\Users\\${MASK}\\Pictures\\${MASK}.jpg`);
    expect(anonymizePath("C:\\Users\\Public\\Documents", ctx)).toBe("C:\\Users\\Public\\Documents");
    expect(anonymizePath("C:\\Windows\\Temp", ctx)).toBe("C:\\Windows\\Temp");
    expect(anonymizePath("D:\\Jeux\\Celeste", { profile: null })).toBe("D:\\Jeux\\Celeste");
  });

  it("nom d'une tuile", () => {
    expect(anonymizeName("C:\\Users\\Arthur\\Documents\\Banque", "Banque", ctx)).toBe(MASK);
    expect(anonymizeName("C:\\Users\\Arthur\\Documents", "Documents", ctx)).toBe("Documents");
    expect(anonymizeName("C:\\Users\\Arthur", "Arthur", ctx)).toBe("%USERPROFILE%");
    expect(anonymizeName("C:\\Users\\Arthur\\AppData", "AppData", ctx)).toBe("AppData");
  });
});
