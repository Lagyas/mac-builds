HS2 – Hypersynth 2 a XGWorks NextGen pro macOS (Apple Silicon i Intel)

1) Presuňte aplikace do slozky Aplikace (Applications).
2) Aplikace nejsou podepsane u Applu. Pri prvnim spusteni macOS hlasi
   "nelze overit / malware". Spustte v Terminalu jednou:
       xattr -cr /Applications/HS2.app "/Applications/HS2 Classic.app" "/Applications/XGWorks NextGen.app"
   a pak aplikaci otevrete (pripadne prave tlacitko > Otevrit).

HS2.app            – syntezator (SwiftUI) + AUv3 plug-in "Lagyas: HS2 – Hypersynth 2"
                     pro Logic / GarageBand / Ableton. Plug-in se zaregistruje po
                     prvnim spusteni HS2.app (overeni: auval -v aumu hs2s Lgys).
                     Obsah HS2: tlacitko "Import HS2 content folder…".
HS2 Classic.app    – stejny panel jako ve Windows verzi.
XGWorks NextGen.app – editor/prehravac .XWS/.MID. Potrebuje data vaseho XGworks V4.0:
                     zkopirujte slozku XG4 do domovske slozky (~/XG4).

Verze: okno About / Help > Copy version info.
