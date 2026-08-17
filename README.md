# 🧹 CLI-NER

> **CLI avanzata, sicura e documentata per la gestione e liberazione dello spazio su disco in macOS.**

---

## 🌟 Caratteristiche Principali

- 🛡️ **Safety-First & Reversibile di Default**:
  - Sposta i file nel **Cestino di macOS** (`~/.Trash`) invece di cancellarli definitivamente.
  - Modalità **Dry-Run attiva di default**: vedi sempre esattamente cosa verrebbe toccato e quanto spazio recupereresti prima di qualsiasi azione.
  - **Blocklist rigorosa**: directory critiche di sistema (`/System`, `/usr`, `/bin`, etc.) e dati utente personali (`~/Documents`, `~/Desktop`, `~/.ssh`, `~/Library/Mail`, `~/Library/Keychains`, etc.) non vengono **MAI** toccati.
  - **Allowlist controllata**: pulisce solo contenuti sicuri autorizzati (`~/Library/Caches/*`, `~/Library/Logs/*`, `/tmp/*`, cache sviluppatore), senza mai eliminare le cartelle radice.
- ⚡ **Performance Elevate**:
  - Scritto in **Rust**, veloce ed efficiente nella scansione ricorsiva del filesystem e nel calcolo delle dimensioni.
- 🛠️ **Supporto Cache Sviluppatore**:
  - **Homebrew**: `brew cleanup -s`, `brew autoremove`
  - **Node.js**: `npm` cache (`~/.npm/_cacache`)
  - **Python**: `pip` cache (`~/Library/Caches/pip`)
  - **Docker**: `docker system prune`
  - **Xcode**: DerivedData, Archives, iOS DeviceSupport (con verifica che Xcode non sia in esecuzione)
- 📝 **Audit Trail & Logging Immutabile**:
  - Ogni operazione di scansione ed esecuzione viene registrata in `~/.cli-ner/logs/` in formato JSON Lines con timestamp, byte liberati, lista dei file ed eventuali errori.
- 🔍 **Analizzatore Disco & Ricerca File Grandi**:
  - Mappatura dello spazio occupato per directory con percentuale di utilizzo.
  - Ricerca ricorsiva di file di grandi dimensioni con soglia personalizzabile (es. `--min-size 500MB`).

---

## 🚀 Installazione

### Requisiti
- macOS (Apple Silicon o Intel)
- Rust & Cargo (1.80+)

### Compilazione dal sorgente
```bash
git clone https://github.com/fabrizio-riccardo/cli-ner.git
cd cli-ner
cargo build --release
```

Il binario compilato si troverà in `target/release/cli-ner`. Puoi copiarlo in `/usr/local/bin` o `~/.local/bin`:
```bash
cp target/release/cli-ner /usr/local/bin/
```

---

## 📖 Utilizzo e Comandi

### 1. `cli-ner scan` — Analisi Spazio Disco
```bash
# Analizza la directory corrente o la home directory
cli-ner scan

# Analizza un percorso specifico
cli-ner scan --path ~/Downloads

# Mostra i top 20 elementi per dimensione
cli-ner scan --top 20

# Ricerca file di grandi dimensioni (>= 500MB)
cli-ner scan --large-files --min-size 500MB

# Output strutturato in formato JSON
cli-ner scan --format json
```

### 2. `cli-ner clean` — Pulizia Sicura Cache e File Temporanei
```bash
# Dry-run (DEFAULT): Mostra cosa verrebbe pulito SENZA toccare alcun file
cli-ner clean

# Esegui la pulizia effettiva (sposta i file nel Cestino macOS)
cli-ner clean --execute

# Pulisce solo una categoria specifica
cli-ner clean --category user-cache --execute
cli-ner clean --category xcode --execute
cli-ner clean --category dev --execute

# Cancellazione permanente (richiede flag esplicito e conferma interattiva)
cli-ner clean --execute --force
```

### 3. `cli-ner dashboard` (oppure `cli-ner report --tui`) — Dashboard Grafica TUI Interattiva
```bash
# Avvia la Dashboard interattiva a terminale per navigare i log e le statistiche
cli-ner dashboard

# Oppure tramite il flag del report
cli-ner report --tui
```
**Controlli Dashboard**:
- `[1] / [2] / [3]` o `[Tab]`: Passa tra Cronologia Operazioni, Dettaglio Singola Operazione e Statistiche per Categoria.
- `[↑] / [↓]` o `[j] / [k]`: Scorri l'elenco delle operazioni o dei singoli file eliminati.
- `[Enter]` o `[d]`: Ispeziona i dettagli della voce selezionata.
- `[q]` o `[Esc]`: Esci dalla dashboard.

### 4. `cli-ner report` — Storico e Audit Operazioni
```bash
# Mostra la tabella delle ultime 10 operazioni
cli-ner report

# Mostra il dettaglio dettagliato dell'ultima operazione
cli-ner report --last

# Esporta in formato JSON
cli-ner report --format json
```

### 5. `cli-ner doctor` — Diagnostica e Verifica Ambiente

```bash
cli-ner doctor
```
Verifica:
- Spazio libero e totale su tutti i dischi montati
- Disponibilità dei tool esterni (Homebrew, npm, pip, Docker, Xcode)
- Stato delle protezioni di sicurezza e permessi

---

## 🛡️ Modello di Sicurezza Dettagliato

### Validazione in 5 Passaggi

Ogni file o directory candidato alla pulizia attraversa il seguente processo prima di qualsiasi modifica:

1. **Verifica Blocklist**: Se il percorso fa parte di cartelle di sistema (`/System`, `/usr`, etc.) o dati personali (`~/.ssh`, `~/Documents`, `~/Desktop`, `~/Library/Mail`), l'operazione viene **immediatamente bloccata**.
2. **Verifica Allowlist**: Il percorso deve rientrare esplicitamente in una categoria autorizzata.
3. **Protezione Root Folder**: Non è consentito cancellare la cartella principale (es. `~/Library/Caches`), ma unicamente i singoli elementi al suo interno.
4. **Symlink Guard**: I symlink non vengono seguiti ciecamente per evitare cancellazioni al di fuori delle cartelle consentite.
5. **Process Safety Check**: Per operazioni sensibili (es. Xcode DerivedData), viene verificato che l'applicazione non sia attualmente in esecuzione.

---

## 🧪 Esecuzione dei Test

Il progetto include una suite completa di unit test e test di integrazione end-to-end:

```bash
# Esegui tutti i test
cargo test

# Esegui con output dettagliato
cargo test -- --nocapture
```

---

## 📄 Licenza

Distribuito sotto licenza MIT.
