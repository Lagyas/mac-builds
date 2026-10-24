HS2 – Hypersynth 2 a XGWorks NextGen pro macOS (Apple Silicon i Intel)

1) Rozbalte zip. Slozku "XGWorks NextGen" presunte celou (i s podslozkami)
   kam chcete, napr. do Aplikace (Applications). HS2.app a HS2 Classic.app
   presunte do Aplikace.
2) Aplikace nejsou podepsane u Applu. Pri prvnim spusteni macOS hlasi
   "nelze overit / malware". Spustte v Terminalu jednou:
       xattr -cr /Applications/HS2.app "/Applications/HS2 Classic.app" "/Applications/XGWorks NextGen"
   a pak aplikaci otevrete.

XGWorks NextGen/     – slozka aplikace, nic se nenastavuje:
   XGWorks NextGen.app   editor/prehravac .XWS/.MID
   XG4/                  data XGworks V4.0 (ws.ini, *.ini, XGworks.exe, XGEditor) – najdou se sama
   Songs/                pisne; Open a Save zacinaji tady
   Settings/             nastaveni aplikace
   Fonts/                volitelne msgothic.ttc pro okno List
   Jina slozka s daty jde zvolit v ⚙ Preferences (tlacitko Default vraci XG4 vedle aplikace).
HS2.app            – syntezator (SwiftUI) + AUv3 plug-in "Lagyas: HS2 – Hypersynth 2".
                     Plug-in se zaregistruje po prvnim spusteni HS2.app (auval -v aumu hs2s Lgys).
HS2 Classic.app    – stejny panel jako ve Windows verzi.

Verze: okno About / Help > Copy version info.
