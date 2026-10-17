# EventPro - Platform Manajemen Siklus Acara dan Tiket dari Awal hingga Akhir

[English](../README.md) | [தமிழ்](README_TA.md) | [简体中文](README_ZH.md) | [हिन्दी](README_HI.md) | Bahasa Indonesia

EventPro adalah platform manajemen acara dan penjualan tiket full-stack yang dirancang untuk menangani seluruh siklus hidup acara, mulai dari perencanaan awal dan publikasi, pemesanan tempat duduk, pembuatan tiket, hingga pengelolaan setelah pemesanan. Platform ini menyediakan alur kerja khusus untuk tiga jenis pengguna utama: Peserta, Penyelenggara Acara, dan Administrator Platform.

![halaman beranda](img/home_page.png)

Repositori ini sepenuhnya menggunakan backend berkinerja tinggi dan aman terhadap kesalahan memori yang ditulis dengan **Rust dan Axum** (`eventpro-rs/`), serta telah menghentikan penggunaan implementasi lama berbasis Express.js secara permanen. Arsitekturnya menerapkan batas transaksi basis data yang ketat, penguncian konkurensi pesimistis pada tingkat baris, serta batas penggunaan memori untuk permintaan multipart, sambil menyajikan template MiniJinja yang dirender di sisi server.

---

## Fitur Utama dan Alur Kerja Pengguna

### Peserta
- **Katalog dan Penemuan Acara**: Jelajahi acara yang telah dipublikasikan dengan filter langsung berdasarkan kategori, kota, dan pencarian teks, yang didukung oleh paginasi di sisi server.
- **Tempat Duduk Bertingkat dan Reservasi**: Pilih jumlah tiket untuk tiket masuk umum atau tingkat tempat duduk khusus, seperti VIP, Reguler, atau Pelajar, dengan penghitungan ulang harga secara langsung.
- **Pemesanan Transaksional**: Tiket dipesan melalui transaksi basis data atomik dengan penguncian baris untuk mencegah penjualan tiket melebihi kapasitas pada acara populer.
- **Tiket dan Pencetakan**: Akses tiket unik yang terdiri dari huruf dan angka melalui tampilan cetak khusus tanpa elemen navigasi, sehingga dapat dicetak secara fisik atau diekspor sebagai PDF.
- **Pembatalan Mandiri**: Batalkan reservasi yang sudah dibuat sebelum acara dimulai. Kapasitas acara dan tingkat tempat duduk akan dipulihkan secara otomatis dan langsung.

### Penyelenggara
- **Pengelolaan Siklus Acara**: Kelola acara melalui seluruh tahapnya: Draf, Dipublikasikan, serta Dibatalkan atau Selesai.
- **Pengaturan Tingkat Tempat Duduk Khusus**: Konfigurasikan tingkat tempat duduk terpisah untuk setiap acara, lengkap dengan nama, kapasitas, dan harga khusus.
- **Pengelolaan Media Banner**: Unggah banner acara dengan validasi konten yang ketat, penetapan nama file acak secara otomatis, serta pembersihan file otomatis ketika acara dihapus.
- **Dasbor Penjualan dan Pendapatan**: Pantau total penjualan tiket, kapasitas yang tersisa, dan pendapatan yang dihasilkan oleh setiap acara.

### Administrator Platform
- **Pengelolaan Seluruh Platform**: Tinjau metrik tingkat sistem, termasuk jumlah pengguna terdaftar, penyelenggara aktif, acara yang telah dipublikasikan, dan total pendapatan kotor platform.
- **Moderasi Acara**: Periksa semua acara dari seluruh penyelenggara, dengan kewenangan administrator untuk mengedit dan menghapus acara.
- **Audit Pengguna**: Pantau pendaftaran akun dan verifikasi penetapan peran di seluruh platform.

---

## Gambaran Umum Arsitektur

| Komponen | Teknologi / Implementasi | Detail |
| :--- | :--- | :--- |
| Runtime dan Bahasa | Rust 1.80+ / runtime asinkron multithread Tokio | Runtime asinkron yang aman terhadap kesalahan memori dan mendukung konkurensi tinggi |
| Kerangka Kerja Web | Axum 0.7 (ekosistem middleware Tower) | Layanan yang dapat dikomposisikan, extractor dengan keamanan tipe, dan middleware khusus |
| Template HTML | MiniJinja | Dikompilasi ke dalam biner dengan escaping HTML kontekstual otomatis |
| Lapisan Basis Data | SQLx 0.8 | Pool koneksi MySQL asinkron dengan jaminan transaksi yang ketat |
| Penyimpanan Sesi | `tower-sessions` | Penyimpanan MySQL untuk penggunaan terdistribusi (tabel `tower_sessions`) |
| Keamanan Kata Sandi | `bcrypt` | Verifikasi standar Blowfish dengan cost 10 (kompatibel dengan hash lama) |
| Arsitektur CSRF | Token kriptografis | Validasi token dalam waktu konstan pada permintaan POST yang mengubah data |
| Unggahan File | Parser multipart streaming dengan batas ukuran | Pemeriksaan magic byte file, batas file 5 MB, dan batas total permintaan 8 MB |
| Penyajian File Statis | `tower-http` ServeDir | Menggunakan `X-Content-Type-Options: nosniff`; file disajikan langsung dari `public/` |

---

## Referensi Konfigurasi

Konfigurasi dikelola melalui variabel lingkungan. Salin `.env.example` menjadi `.env` di direktori root proyek atau di dalam `eventpro-rs/`:

```bash
cp .env.example .env
```

| Variabel | Persyaratan | Default | Kegunaan |
| :--- | :--- | :--- | :--- |
| `DATABASE_URL` | Wajib (atau gunakan variabel `DB_*` secara terpisah) | Tidak ada | String koneksi MySQL: `mysql://user:pass@host:port/database` |
| `SESSION_SECRET` | Wajib | Tidak ada | Kunci rahasia untuk menandatangani sesi (minimal 16 karakter) |
| `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME` | Alternatif cadangan | Tidak ada | Parameter koneksi terpisah yang digunakan ketika `DATABASE_URL` tidak ditetapkan |
| `PORT` | Opsional | `3001` | Port tempat server HTTP Axum menerima koneksi |
| `HOST` | Opsional | `127.0.0.1` | Alamat antarmuka jaringan yang digunakan untuk binding |
| `APP_ENV` | Opsional | `development` | Atur ke `production` untuk mewajibkan `Secure=true` pada cookie sesi |
| `PUBLIC_DIR` | Opsional | `public` | Direktori yang berisi CSS, JavaScript sisi klien, dan media statis |
| `UPLOAD_DIR` | Opsional | `PUBLIC_DIR/uploads` | Lokasi sistem berkas untuk menyimpan unggahan gambar banner |

*Catatan keamanan: Pemuat konfigurasi Rust menggunakan formatter debug khusus yang menyamarkan `DATABASE_URL` dan `SESSION_SECRET` sebagai `[REDACTED]`. Nilai rahasia tidak pernah ditulis ke output standar atau sistem agregasi log.*

---

## Memulai

### 1. Prasyarat
- **MySQL Server 8.0+** yang berjalan pada port `3306`
- **Rust Toolchain 1.80+** (Cargo dan rustc)
- **Node.js 20+** (opsional, hanya diperlukan untuk menjalankan rangkaian benchmark mandiri di `benchmarks/`)

### 2. Menyiapkan Basis Data
Inisialisasi skema basis data dan data contoh menggunakan klien MySQL pilihan Anda:

```bash
# Terapkan tabel skema dan constraint
mysql -u root -p < database/schema.sql

# Muat kategori awal, acara contoh, tingkat tempat duduk, dan akun demonstrasi
mysql -u root -p event_lifecycle < database/seed.sql
```

Jika belum tersedia, backend Rust akan membuat tabel pengelolaan sesi (`tower_sessions`) secara otomatis saat dijalankan. Tabel inti aplikasi (`users`, `events`, `categories`, `seat_types`, `bookings`, `tickets`) tidak diubah sama sekali.

### 3. Menjalankan Backend Rust

Dari direktori root repositori:

```bash
cd eventpro-rs
cargo run
```

Server akan dikompilasi, terhubung ke MySQL, memverifikasi penyimpanan sesi, lalu mendengarkan koneksi di `http://localhost:3001` (atau port yang ditetapkan melalui `PORT`).

Endpoint pemeriksaan operasional:
- **Pemeriksaan liveness**: `GET http://localhost:3001/health` (mengembalikan status JSON)
- **Pemeriksaan readiness**: `GET http://localhost:3001/ready` (menguji eksekusi kueri MySQL secara langsung)

---

## Akun Demonstrasi

Basis data seed menyertakan akun yang telah dikonfigurasi untuk mewakili setiap jenis pengguna:

| Peran | Email | Kata Sandi | Hak Akses |
| :--- | :--- | :--- | :--- |
| Administrator | `admin@eventpro.com` | `Admin@1234` | Dasbor lengkap platform, moderasi acara, analitik global |
| Penyelenggara | `bob@events.com` | `Organizer@1` | Membuat dan mengedit acara, mengunggah banner, mengatur tingkat tiket |
| Peserta | `charlie@mail.com` | `User@1234` | Menjelajahi katalog, memilih tempat duduk, memesan tiket, mengakses tiket, membatalkan pemesanan |

*(Kata sandi lama dari seed pengembangan sebelumnya, yaitu `admin123`, `org123`, dan `user123`, juga didukung oleh bcrypt hasher.)*

## Arsitektur Keamanan dan Konkurensi

### 1. Logout POST yang Dilindungi CSRF
Penghancuran sesi hanya diizinkan melalui `POST /auth/logout` dan memerlukan token CSRF yang valid. Bilah navigasi atas dan sidebar dasbor menampilkan formulir semantik dengan kolom tersembunyi `csrf_token`.
- Mengirim formulir logout dengan token yang valid akan menghapus sesi dari MySQL dan mengarahkan pengguna ke halaman beranda publik.
- Permintaan dengan token CSRF yang hilang atau dipalsukan akan ditolak dengan HTTP 403 Forbidden; sesi aktif pengguna tetap terautentikasi.
- Permintaan ke `GET /auth/logout` menerima respons HTTP 405 Method Not Allowed. Hal ini mencegah penghentian sesi yang tidak disengaja oleh crawler web, pengambil tautan otomatis, atau navigasi riwayat browser.

### 2. Mitigasi Session Fixation
Setiap kali login pengguna atau pendaftaran akun berhasil, `session.cycle_id()` dipanggil tepat sebelum detail pengguna dimasukkan ke sesi. Proses ini menghasilkan kunci sesi baru dan menghilangkan kerentanan session fixation.

### 3. Pengendalian Konkurensi dan Integritas Transaksi
Penjualan tiket dengan permintaan tinggi dapat menimbulkan race condition serius ketika beberapa peserta mencoba memesan kursi terakhir yang tersedia pada saat bersamaan:
- **Penguncian Pesimistis**: Handler pemesanan memulai transaksi dan menerapkan kunci `SELECT ... FOR UPDATE` pada baris acara serta baris tingkat tempat duduk yang dipilih.
- **Pemeriksaan Kapasitas Ketat**: Pemeriksaan kapasitas dilakukan di dalam transaksi yang terkunci. Jika jumlah tiket yang diminta melebihi kursi yang tersedia, transaksi dibatalkan dengan rollback tanpa menyisakan catatan parsial.
- **Konsistensi Tingkat Tempat Duduk**: Jika acara memiliki beberapa tingkat tempat duduk, pemesanan tanpa pilihan tingkat yang valid akan ditolak sebelum alokasi dilakukan.
- **Pembatalan Atomik**: Pembatalan pemesanan memperbarui catatan pemesanan dan memulihkan kapasitas acara serta tingkat tempat duduk dalam satu batas transaksi.
- **Percobaan Ulang Jika Terjadi Tabrakan**: Kode tiket unik (`EVT-{event_id}-BKG-{booking_id}-{random_hex}`) diperiksa terhadap constraint keunikan, dengan logika percobaan ulang otomatis.

### 4. Unggahan Multipart dengan Batas dan Pembatasan Sistem Berkas
Unggahan banner acara ditangani oleh parser streaming terisolasi dengan batas yang tegas:
- **Batas Isi Permintaan**: Endpoint pembuatan dan pengeditan acara memiliki batas total permintaan 8 MB (`MAX_EVENT_FORM_BODY_SIZE`) untuk mencegah denial-of-service melalui permintaan tanpa batas.
- **Batas Aliran File**: Ukuran file banner dibatasi hingga 5 MB (`MAX_BANNER_SIZE`). Streaming dihentikan dan kesalahan ditangani dengan benar jika batas ini terlampaui.
- **Batas Kolom Teks**: Kolom formulir non-file dibatasi hingga 64 KB (`MAX_TEXT_FIELD_SIZE`) untuk mencegah kehabisan memori akibat muatan teks yang terlalu besar.
- **Pemeriksaan Konten Magic Byte**: File diperiksa berdasarkan signature magic byte standar (JPEG, PNG, WebP, GIF). Ekstensi file dan header MIME yang diberikan pengguna tidak dipercaya.
- **Nama File Aman**: File yang diunggah diberi nama heksadesimal acak yang dihasilkan secara kriptografis (`event-{timestamp}-{hex}.{ext}`). Urutan path traversal (`..`, `/`, `\`) tidak dapat mengubah direktori tujuan.
- **Pembersihan Siklus Hidup File**: Jika penyisipan data ke basis data gagal setelah file diunggah, file gambar yatim segera dihapus. Ketika acara dihapus, banner dinamisnya juga dihapus, sementara aset seed bawaan (`event-*.jpg`) tetap dipertahankan dengan aman.
- **Perlindungan Content Sniffing**: File yang diunggah disajikan dengan header `X-Content-Type-Options: nosniff` untuk mencegah browser menjalankan muatan HTML atau skrip yang disamarkan.

### 5. Alur Pembayaran Simulasi
Halaman checkout (`GET /bookings/payment`) dan endpoint konfirmasi (`POST /bookings/confirm`) menyimulasikan proses pembayaran e-commerce. Tidak ada penyedia pembayaran eksternal seperti Stripe atau PayPal yang dipanggil. Setelah pemesanan dikonfirmasi, kode tiket valid langsung diterbitkan untuk keperluan demonstrasi platform.

## Pertimbangan Deployment

### Catatan Deployment Produksi
1. **Pengaturan Reverse Proxy**: Untuk produksi, jalankan EventPro di belakang Nginx, Caddy, atau cloud load balancer yang menangani terminasi TLS dan meneruskan lalu lintas ke `127.0.0.1:3001`.
2. **Keamanan Cookie**: Tetapkan `APP_ENV=production` agar lapisan sesi menerapkan `Secure=true` dan `SameSite=Lax` secara otomatis pada cookie.
3. **Volume Unggahan Persisten**: Dalam lingkungan berbasis container seperti Docker atau Kubernetes, petakan `UPLOAD_DIR` ke volume bersama yang persisten agar media yang diunggah tetap tersedia setelah pod dimulai ulang.
4. **Penskalaan Horizontal**: Tabel sesi berbasis basis data (`tower_sessions`) memungkinkan beberapa instans backend berjalan di belakang load balancer round-robin tanpa memerlukan sticky session.

## Struktur Direktori

```text
.
├── benchmarks/               <- Rangkaian benchmark performa dan verifikasi mandiri
│   ├── package.json          <- Dependensi alat benchmark (autocannon, mysql2)
│   ├── results/              <- Hasil pengukuran benchmark dan data mentah yang dipertahankan
│   └── *.js                  <- Harness benchmark dan pengujian kebenaran
├── database/                 <- Skema SQL basis data, data seed, dan migrasi
│   ├── schema.sql            <- Tabel aplikasi dan constraint relasional
│   ├── seed.sql              <- Kategori, acara demonstrasi, dan pengguna
│   └── migrations/           <- Penambahan indeks dan optimisasi
├── public/                   <- Aset klien dan penyimpanan media
│   ├── css/                  <- Gaya aplikasi
│   ├── js/                   <- Utilitas skrip sisi klien
│   └── uploads/              <- Banner acara yang diunggah
├── eventpro-rs/              <- Layanan backend Rust / Axum
│   ├── Cargo.toml            <- Konfigurasi crate dan dependensi
│   ├── src/
│   │   ├── main.rs           <- Entry point server, penanganan sinyal, dan binding listener
│   │   ├── lib.rs            <- Definisi modul utama library
│   │   ├── config.rs         <- Pemuatan lingkungan dan penyamaran kredensial
│   │   ├── csrf.rs           <- Pembuatan token CSRF dan validasi waktu konstan
│   │   ├── error.rs          <- Penanganan error terpusat dan pemetaan kode status
│   │   ├── middleware.rs     <- Autentikasi peran dan penjagaan sesi
│   │   ├── upload.rs         <- Parser multipart berbatas dan pemeriksaan magic byte
│   │   ├── models/           <- Model basis data (User, Event, Booking, SeatType, Ticket, Category)
│   │   └── routes/           <- Handler rute (events, bookings, auth, tickets, admin, health)
│   ├── templates/            <- Tampilan HTML sisi server dengan MiniJinja
│   └── tests/                <- Rangkaian pengujian integrasi dan kebenaran yang komprehensif
└── README.md                 <- Dokumentasi proyek yang terkonsolidasi
```

## Prosedur Pengujian dan Verifikasi

Basis kode Rust memiliki cakupan pengujian yang komprehensif untuk unit test, rendering template, dan integrasi basis data.

Jalankan rangkaian verifikasi dari direktori `eventpro-rs/`:

```bash
cd eventpro-rs

# 1. Verifikasi format kode
cargo fmt --check

# 2. Pemeriksaan linter ketat
cargo clippy --all-targets -- -D warnings

# 3. Jalankan pengujian standar
cargo test

# 4. Rangkaian integrasi MySQL dengan basis data nyata secara ketat (97 pengujian)
# Linux / macOS:
STRICT_DB_TESTS=1 cargo test -j 2

# Windows PowerShell:
& { $env:STRICT_DB_TESTS='1'; cargo test -j 2 }
```

### Jaminan Pengujian Basis Data Ketat
Saat `STRICT_DB_TESTS=1` ditetapkan:
- Pengujian terhubung ke instans MySQL aktif pada port 3306.
- Jika basis data tidak dapat dijangkau atau konfigurasinya salah, rangkaian pengujian langsung berhenti dengan panic, alih-alih melewatkan pengujian integrasi secara diam-diam.
- Transaksi nyata, simulasi pemesanan bersamaan, percobaan ulang saat kode bertabrakan, serta siklus hidup penyimpanan banner benar-benar diuji terhadap MySQL.

## Lisensi

Proyek ini menggunakan Lisensi MIT. Lihat [LICENSE](LICENSE) untuk detail lebih lanjut.
