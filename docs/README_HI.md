# EventPro - संपूर्ण इवेंट लाइफ़साइकल और टिकटिंग प्लेटफ़ॉर्म

[English](../README.md) | [தமிழ்](README_TA.md) | [简体中文](README_ZH.md) | हिन्दी | [Bahasa Indonesia](README_ID.md)

EventPro एक फुल-स्टैक इवेंट प्रबंधन और टिकटिंग प्लेटफ़ॉर्म है, जिसे किसी इवेंट के पूरे जीवनचक्र को संभालने के लिए बनाया गया है। इसमें शुरुआती योजना और प्रकाशन से लेकर सीट आरक्षण, टिकट निर्माण और बुकिंग के बाद के प्रबंधन तक की प्रक्रियाएँ शामिल हैं। यह प्लेटफ़ॉर्म तीन प्रमुख उपयोगकर्ता भूमिकाओं के अनुसार कार्यप्रवाह उपलब्ध कराता है: सहभागी (Attendees), इवेंट आयोजक (Event Organizers) और प्लेटफ़ॉर्म प्रशासक (Platform Administrators)।

![होम पेज](img/home_page.png)

यह रिपॉज़िटरी पूरी तरह **Rust और Axum** (`eventpro-rs/`) में लिखे गए उच्च-प्रदर्शन और मेमोरी-सुरक्षित बैकएंड पर आधारित है। पुराने Express.js बैकएंड को स्थायी रूप से हटा दिया गया है। आर्किटेक्चर में डेटाबेस ट्रांज़ैक्शन की स्पष्ट सीमाएँ, पंक्ति-स्तर पर pessimistic concurrency locks और multipart अनुरोधों के लिए सीमित मेमोरी उपयोग लागू किया गया है। यह MiniJinja टेम्पलेट्स से सर्वर-साइड रेंडर किए गए पेज भी उपलब्ध कराता है।

---

## मुख्य सुविधाएँ और उपयोगकर्ता कार्यप्रवाह

### सहभागी (Attendees)
- **इवेंट कैटलॉग और खोज**: प्रकाशित इवेंट देखें और श्रेणी, शहर तथा टेक्स्ट खोज के आधार पर उन्हें फ़िल्टर करें। सर्वर-साइड pagination से परिणाम व्यवस्थित रूप में दिखते हैं।
- **विभिन्न श्रेणियों की सीटें और आरक्षण**: सामान्य प्रवेश या कस्टम सीट श्रेणियों (जैसे VIP, Regular या Student) में टिकटों की संख्या चुनें। कीमत तुरंत दोबारा गणना की जाती है।
- **ट्रांज़ैक्शन-आधारित बुकिंग**: लोकप्रिय इवेंट में उपलब्ध सीटों से अधिक बुकिंग रोकने के लिए टिकटों का आरक्षण atomic database transactions और row locking के भीतर किया जाता है।
- **टिकट पास और प्रिंटिंग**: अद्वितीय अक्षरांकीय टिकट पास देखें। प्रिंट के लिए अलग पेज उपलब्ध है, जिसमें नेविगेशन तत्व नहीं होते, ताकि टिकट को कागज़ पर प्रिंट किया जा सके या PDF के रूप में निर्यात किया जा सके।
- **स्वयं बुकिंग रद्द करना**: इवेंट शुरू होने से पहले मौजूदा आरक्षण रद्द करें। रद्द होने पर इवेंट और सीट श्रेणी की उपलब्ध क्षमता अपने-आप बहाल हो जाती है।

### आयोजक (Organizers)
- **इवेंट जीवनचक्र प्रबंधन**: इवेंट को उसके पूरे जीवनचक्र में प्रबंधित करें: Draft, Published और Cancelled/Completed।
- **कस्टम सीट श्रेणियाँ**: हर इवेंट के लिए अलग-अलग सीट श्रेणियाँ, उनके नाम, सीट क्षमता और कीमत निर्धारित करें।
- **बैनर मीडिया प्रबंधन**: इवेंट बैनर अपलोड करें। अपलोड की गई फ़ाइल की सामग्री की सख़्त जाँच होती है, उसे स्वचालित रूप से यादृच्छिक फ़ाइल नाम दिया जाता है और इवेंट हटाने पर संबंधित फ़ाइल साफ़ की जाती है।
- **बिक्री और राजस्व डैशबोर्ड**: प्रत्येक इवेंट के कुल टिकट बिक्री, शेष क्षमता और उससे हुई आय पर नज़र रखें।

### प्लेटफ़ॉर्म प्रशासक (Platform Administrators)
- **पूरे प्लेटफ़ॉर्म का प्रबंधन**: कुल पंजीकृत उपयोगकर्ता, सक्रिय आयोजक, प्रकाशित इवेंट और प्लेटफ़ॉर्म का कुल राजस्व जैसे आँकड़े देखें।
- **इवेंट मॉडरेशन**: सभी आयोजकों के इवेंट देखें और उन्हें संपादित या हटाने के प्रशासनिक अधिकारों का उपयोग करें।
- **उपयोगकर्ता ऑडिट**: खाते के पंजीकरण पर नज़र रखें और प्लेटफ़ॉर्म पर उपयोगकर्ता भूमिकाओं के आवंटन की पुष्टि करें।

---

## आर्किटेक्चर का अवलोकन

| घटक | तकनीक / कार्यान्वयन | विवरण |
| :--- | :--- | :--- |
| रनटाइम और भाषा | Rust 1.80+ / Tokio multi-threaded async runtime | मेमोरी-सुरक्षित और उच्च concurrency वाला asynchronous runtime |
| वेब फ़्रेमवर्क | Axum 0.7 (Tower middleware ecosystem) | संयोज्य सेवाएँ, type-safe extractors और कस्टम middleware |
| HTML टेम्पलेटिंग | MiniJinja | बाइनरी में संकलित; संदर्भ के अनुसार HTML escaping स्वचालित रूप से करता है |
| डेटाबेस लेयर | SQLx 0.8 | सख़्त transactional गारंटी वाला asynchronous MySQL connection pool |
| सेशन स्टोर | `tower-sessions` | MySQL में संग्रहीत साझा सेशन डेटा (`tower_sessions` तालिका) |
| पासवर्ड सुरक्षा | `bcrypt` | मानक Blowfish cost 10 सत्यापन; पुराने हैश के साथ संगत |
| CSRF सुरक्षा | क्रिप्टोग्राफ़िक टोकन | डेटा बदलने वाले POST अनुरोधों पर constant-time token validation |
| फ़ाइल अपलोड | सीमित streaming multipart parser | Magic-byte द्वारा फ़ाइल जाँच, प्रति फ़ाइल 5 MB की सीमा और पूरे अनुरोध पर 8 MB की सीमा |
| स्थिर फ़ाइलें उपलब्ध कराना | `tower-http` ServeDir | `X-Content-Type-Options: nosniff`; फ़ाइलें सीधे `public/` से उपलब्ध कराई जाती हैं |

---

## कॉन्फ़िगरेशन संदर्भ

कॉन्फ़िगरेशन को environment variables के ज़रिए नियंत्रित किया जाता है। प्रोजेक्ट के root या `eventpro-rs/` फ़ोल्डर में `.env.example` की कॉपी बनाकर उसे `.env` नाम दें:

```bash
cp .env.example .env
```

| वेरिएबल | आवश्यकता | डिफ़ॉल्ट | उद्देश्य |
| :--- | :--- | :--- | :--- |
| `DATABASE_URL` | अनिवार्य (`DB_*` वेरिएबल अलग-अलग देने का विकल्प भी है) | कोई नहीं | MySQL connection string: `mysql://user:pass@host:port/database` |
| `SESSION_SECRET` | अनिवार्य | कोई नहीं | सेशन साइन करने के लिए गुप्त कुंजी; कम-से-कम 16 अक्षर आवश्यक हैं |
| `DB_HOST`, `DB_PORT`, `DB_USER`, `DB_PASSWORD`, `DB_NAME` | वैकल्पिक fallback | कोई नहीं | `DATABASE_URL` सेट न होने पर अलग-अलग connection parameters के रूप में उपयोग होते हैं |
| `PORT` | वैकल्पिक | `3001` | वह पोर्ट जिस पर Axum HTTP सर्वर सुनता है |
| `HOST` | वैकल्पिक | `127.0.0.1` | नेटवर्क इंटरफ़ेस जिससे सर्वर bind होगा |
| `APP_ENV` | वैकल्पिक | `development` | `production` सेट करने पर session cookies के लिए `Secure=true` लागू होता है |
| `PUBLIC_DIR` | वैकल्पिक | `public` | CSS, client-side JavaScript और स्थिर मीडिया वाली निर्देशिका |
| `UPLOAD_DIR` | वैकल्पिक | `PUBLIC_DIR/uploads` | अपलोड किए गए बैनर चित्रों को संग्रहीत करने का फ़ाइल सिस्टम पथ |

*सुरक्षा नोट: Rust का configuration loader एक कस्टम debug formatter का उपयोग करता है, जो `DATABASE_URL` और `SESSION_SECRET` को `[REDACTED]` के रूप में छिपाता है। गुप्त मान standard output या log aggregators में नहीं लिखे जाते।*

---

## शुरुआत कैसे करें

### 1. आवश्यकताएँ
- पोर्ट `3306` पर चल रहा **MySQL Server 8.0+**
- **Rust Toolchain 1.80+** (`Cargo` और `rustc` सहित)
- **Node.js 20+** (वैकल्पिक; केवल `benchmarks/` में मौजूद स्वतंत्र benchmark suite चलाने के लिए आवश्यक)

### 2. डेटाबेस सेटअप
अपने पसंदीदा MySQL client से डेटाबेस स्कीमा और नमूना डेटा प्रारंभ करें:

```bash
# स्कीमा की तालिकाएँ और constraints लागू करें
mysql -u root -p < database/schema.sql

# शुरुआती श्रेणियाँ, नमूना इवेंट, सीट श्रेणियाँ और demo खाते लोड करें
mysql -u root -p event_lifecycle < database/seed.sql
```

यदि `tower_sessions` सेशन प्रबंधन तालिका पहले से मौजूद नहीं है, तो Rust बैकएंड स्टार्टअप के दौरान इसे अपने-आप बना देता है। मुख्य application tables (`users`, `events`, `categories`, `seat_types`, `bookings`, `tickets`) में कोई बदलाव नहीं किया जाता।

### 3. Rust बैकएंड चलाना

रिपॉज़िटरी के root से:

```bash
cd eventpro-rs
cargo run
```

सर्वर compile होगा, MySQL से जुड़ेगा, session store की जाँच करेगा और `http://localhost:3001` पर (या आपके कॉन्फ़िगर किए गए `PORT` पर) अनुरोधों को सुनना शुरू करेगा।

सर्वर की स्थिति जाँचने वाले endpoints:
- **Liveness probe**: `GET http://localhost:3001/health` (JSON status लौटाता है)
- **Readiness probe**: `GET http://localhost:3001/ready` (वास्तविक MySQL query चलाकर कनेक्शन की जाँच करता है)

---

## प्रदर्शन के लिए डेमो खाते

Seed database में तीनों उपयोगकर्ता भूमिकाओं के लिए पहले से कॉन्फ़िगर किए गए खाते हैं:

| भूमिका | ईमेल | पासवर्ड | अनुमतियाँ |
| :--- | :--- | :--- | :--- |
| प्रशासक | `admin@eventpro.com` | `Admin@1234` | पूरा प्लेटफ़ॉर्म डैशबोर्ड, इवेंट मॉडरेशन और वैश्विक विश्लेषण |
| आयोजक | `bob@events.com` | `Organizer@1` | इवेंट बनाना और संपादित करना, बैनर अपलोड और टिकट श्रेणियाँ निर्धारित करना |
| सहभागी | `charlie@mail.com` | `User@1234` | इवेंट खोजना, सीट चुनना, बुकिंग, टिकट देखना और बुकिंग रद्द करना |

*(पहले के development seed में इस्तेमाल हुए पुराने पासवर्ड `admin123`, `org123` और `user123` भी bcrypt hasher द्वारा समर्थित हैं।)*

## सुरक्षा और concurrency आर्किटेक्चर

### 1. CSRF-सुरक्षित POST लॉगआउट
सेशन समाप्त करने की अनुमति केवल `POST /auth/logout` के माध्यम से है और इसके लिए वैध CSRF token आवश्यक है। मुख्य navigation bar और dashboard sidebar, दोनों में छिपे हुए `csrf_token` फ़ील्ड वाले semantic forms उपलब्ध हैं।
- वैध टोकन के साथ logout form सबमिट करने पर MySQL में सेशन नष्ट हो जाता है और उपयोगकर्ता को सार्वजनिक होम पेज पर भेज दिया जाता है।
- CSRF टोकन न होने या जाली टोकन देने पर अनुरोध `HTTP 403 Forbidden` के साथ अस्वीकार होता है; उपयोगकर्ता का सक्रिय सेशन लॉग-इन स्थिति में बना रहता है।
- `GET /auth/logout` को एक्सेस करने के प्रयास पर `HTTP 405 Method Not Allowed` मिलता है। इससे web crawlers, link pre-fetchers या browser history navigation के कारण अनजाने में सेशन समाप्त होने से बचाव होता है।

### 2. Session fixation से बचाव
हर सफल लॉगिन और खाता पंजीकरण के दौरान उपयोगकर्ता की जानकारी सेशन में भरने से ठीक पहले `session.cycle_id()` चलाया जाता है। इससे नई session key बनती है और session fixation की कमज़ोरी से बचाव होता है।

### 3. Concurrency नियंत्रण और transaction integrity
जब कई सहभागी एक ही समय पर बची हुई आख़िरी सीटें बुक करते हैं, तो टिकट बिक्री में गंभीर race conditions हो सकती हैं:
- **Pessimistic locking**: बुकिंग handler एक transaction शुरू करता है और इवेंट की row तथा चुनी गई seat tier की row, दोनों पर `SELECT ... FOR UPDATE` लॉक लगाता है।
- **क्षमता की सख़्त जाँच**: सीट क्षमता की जाँच locked transaction के भीतर होती है। यदि अनुरोधित टिकटों की संख्या उपलब्ध सीटों से अधिक हो, तो transaction बिना अधूरे रिकॉर्ड छोड़े rollback हो जाता है।
- **Seat tier का मिलान**: यदि किसी इवेंट में सीट श्रेणियाँ परिभाषित हैं, तो वैध श्रेणी चुने बिना आरक्षण का प्रयास सीट आवंटन से पहले अस्वीकार कर दिया जाता है।
- **Atomic cancellation**: बुकिंग रद्द करते समय booking record अपडेट करना और इवेंट तथा सीट श्रेणी की क्षमता बहाल करना एक ही transaction के भीतर होता है।
- **Collision पर पुनः प्रयास**: अद्वितीय टिकट कोड (`EVT-{event_id}-BKG-{booking_id}-{random_hex}`) को uniqueness constraints के विरुद्ध जाँचा जाता है और टकराव होने पर अपने-आप दोबारा प्रयास किया जाता है।

### 4. सीमित multipart अपलोड और फ़ाइल सिस्टम सुरक्षा
इवेंट बैनर अपलोड एक अलग streaming parser के ज़रिए संसाधित किए जाते हैं, जिस पर निश्चित सीमाएँ लागू हैं:
- **अनुरोध के कुल आकार की सीमा**: इवेंट बनाने और संपादित करने वाले endpoints पर कुल अनुरोध आकार की सीमा 8 MB (`MAX_EVENT_FORM_BODY_SIZE`) है। इससे असीमित आकार के अनुरोध भेजकर denial-of-service करने का जोखिम घटता है।
- **फ़ाइल स्ट्रीम की सीमा**: बैनर फ़ाइल अधिकतम 5 MB (`MAX_BANNER_SIZE`) की हो सकती है। यह सीमा पार होने पर streaming रुक जाती है और उचित त्रुटि मिलती है।
- **टेक्स्ट फ़ील्ड की सीमा**: फ़ाइल न होने वाले form fields अधिकतम 64 KB (`MAX_TEXT_FIELD_SIZE`) तक सीमित हैं, ताकि बहुत बड़े टेक्स्ट से मेमोरी समाप्त होने का जोखिम न हो।
- **Magic-byte द्वारा सामग्री की जाँच**: फ़ाइलों में JPEG, PNG, WebP और GIF के मानक magic-byte signatures जाँचे जाते हैं। उपयोगकर्ता द्वारा दिया गया file extension और MIME header विश्वसनीय नहीं माना जाता।
- **सुरक्षित फ़ाइल नाम**: अपलोड की गई फ़ाइलों को क्रिप्टोग्राफ़िक रूप से यादृच्छिक hexadecimal नाम (`event-{timestamp}-{hex}.{ext}`) दिए जाते हैं। Path traversal sequences (`..`, `/`, `\`) के ज़रिए destination directory बदली नहीं जा सकती।
- **जीवनचक्र के दौरान सफ़ाई**: अपलोड के बाद database insert विफल होने पर अनाथ image file तुरंत हटाई जाती है। इवेंट हटाने पर उसका dynamic banner भी हटाया जाता है, जबकि डिफ़ॉल्ट seed assets (`event-*.jpg`) सुरक्षित रखे जाते हैं।
- **Content sniffing से सुरक्षा**: अपलोड की गई फ़ाइलें `X-Content-Type-Options: nosniff` headers के साथ दी जाती हैं, ताकि ब्राउज़र छिपे हुए HTML या script payload को निष्पादित न करें।

### 5. नकली भुगतान (Mock Payment) कार्यप्रवाह
Checkout स्क्रीन (`GET /bookings/payment`) और पुष्टि endpoint (`POST /bookings/confirm`) ई-कॉमर्स checkout का अनुकरण करते हैं। Stripe या PayPal जैसे बाहरी payment processors का उपयोग नहीं होता। पुष्टि की गई बुकिंग के लिए प्लेटफ़ॉर्म प्रदर्शन हेतु वैध टिकट कोड तुरंत जारी किए जाते हैं।

## Deployment संबंधी विचार

### Production deployment के लिए सुझाव
1. **Reverse proxy सेटअप**: Production में EventPro को Nginx, Caddy या ऐसे cloud load balancer के पीछे चलाएँ जो TLS समाप्त करता हो और अनुरोधों को `127.0.0.1:3001` पर भेजता हो।
2. **Cookie सुरक्षा**: `APP_ENV=production` सेट करें, ताकि session layer cookies पर `Secure=true` और `SameSite=Lax` लागू करे।
3. **स्थायी upload storage**: Docker या Kubernetes जैसे containerized वातावरण में `UPLOAD_DIR` को persistent shared volume पर मैप करें, ताकि pod restart के बाद भी अपलोड की गई मीडिया उपलब्ध रहे।
4. **Horizontal scaling**: डेटाबेस-आधारित session table (`tower_sessions`) के कारण कई backend instances को round-robin load balancer के पीछे चलाया जा सकता है; sticky sessions की आवश्यकता नहीं होती।

## निर्देशिका संरचना

```text
.
├── benchmarks/               <- स्वतंत्र performance benchmarking और verification suite
│   ├── package.json          <- Benchmark tooling dependencies (autocannon, mysql2)
│   ├── results/              <- संरक्षित benchmark माप और raw data
│   └── *.js                  <- Benchmark harnesses और correctness tests
├── database/                 <- Database SQL schema, seed data और migrations
│   ├── schema.sql            <- Application tables और relational constraints
│   ├── seed.sql              <- श्रेणियाँ, demo इवेंट और उपयोगकर्ता
│   └── migrations/           <- अतिरिक्त indexes और optimizations
├── public/                   <- Client assets और media storage
│   ├── css/                  <- Application styles
│   ├── js/                   <- Client-side script utilities
│   └── uploads/              <- अपलोड किए गए इवेंट बैनर
├── eventpro-rs/              <- Rust / Axum Backend Service
│   ├── Cargo.toml            <- Crate configuration और dependencies
│   ├── src/
│   │   ├── main.rs           <- Server entry point, signal handling और listener binding
│   │   ├── lib.rs            <- Root library module definitions
│   │   ├── config.rs         <- Environment loading और credentials masking
│   │   ├── csrf.rs           <- CSRF token निर्माण और constant-time validation
│   │   ├── error.rs          <- केंद्रीकृत error handling और status code mapping
│   │   ├── middleware.rs     <- Role authentication और session guards
│   │   ├── upload.rs         <- सीमित multipart parser और magic-byte checks
│   │   ├── models/           <- Database models (User, Event, Booking, SeatType, Ticket, Category)
│   │   └── routes/           <- Route handlers (events, bookings, auth, tickets, admin, health)
│   ├── templates/            <- MiniJinja द्वारा server-rendered HTML views
│   └── tests/                <- Integration और correctness test suites
└── README.md                 <- समेकित project documentation
```

## Testing और verification प्रक्रिया

Rust codebase में unit tests, template rendering और database integration के लिए व्यापक परीक्षण शामिल हैं।

`eventpro-rs/` निर्देशिका से verification suite चलाएँ:

```bash
cd eventpro-rs

# 1. Formatting की जाँच
cargo fmt --check

# 2. सख़्त linter जाँच
cargo clippy --all-targets -- -D warnings

# 3. सामान्य test run
cargo test

# 4. वास्तविक MySQL database के साथ सख़्त integration suite (97 tests)
# Linux / macOS:
STRICT_DB_TESTS=1 cargo test -j 2

# Windows PowerShell:
& { $env:STRICT_DB_TESTS='1'; cargo test -j 2 }
```

### सख़्त डेटाबेस परीक्षण की गारंटी
जब `STRICT_DB_TESTS=1` सेट किया जाता है:
- टेस्ट पोर्ट 3306 पर चल रहे वास्तविक MySQL instance से जुड़ते हैं।
- यदि डेटाबेस उपलब्ध नहीं है या उसका कॉन्फ़िगरेशन गलत है, तो integration tests को चुपचाप छोड़ने के बजाय test suite तुरंत panic के साथ रुक जाती है।
- वास्तविक transactions, concurrent booking simulation, ticket-code collision retries और banner storage lifecycle की MySQL के विरुद्ध सक्रिय रूप से जाँच होती है।

## लाइसेंस

यह प्रोजेक्ट MIT License का उपयोग करता है। अधिक जानकारी के लिए [LICENSE](LICENSE) देखें।
