A riposo Loglem usa 7,6 MB:

~4 MB sono il programma stesso caricato in memoria. Sono pagine del file su disco, che il kernel può liberare quando gli serve spazio. Riscrivere le librerie riduce questa parte, ma al massimo di 1,5–2 MB.
~1,6 MB sono libc, che è condivisa con tutti gli altri processi del server.
1,3 MB sono la memoria propria vera, ed è già pochissima.
I picchi da 30–40 MB invece non vengono da una libreria Rust:

il backtrace è una funzione della libreria standard, chiamata dove non serve;
la memoria che non torna indietro è dell'allocatore di glibc, che è in C;
il picco delle statistiche viene dall'algoritmo, che tiene in memoria ogni IP come testo.
Riscrivendo tutte le librerie guadagneresti circa 2 MB. Sistemando i punti 1 e 2 del documento ne guadagni circa 30.

Le librerie che si possono riscrivere o togliere
Libreria	Peso	Cosa fare	Guadagno
ureq + rustls + ring (TLS)	~0,7 MB	Toglierle e chiamare curl	Il più alto tra le librerie. Fatica bassa
clap (comandi)	~0,4 MB	Parser scritto a mano	Medio. Fatica alta (1.500 righe di cli.rs)
serde_json Value	~0,45 MB	Struct dirette al posto di Value	Piccolo sul binario, grande sul picco delle statistiche
chrono	~30 KB	Funzioni di data scritte a mano	Trascurabile, non vale la pena