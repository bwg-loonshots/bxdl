# Java 21 런타임 선정과 공급자 검증

2026-10-19 · BXDL macOS arm64 개발 후보. [사용법](../docs/runtime.md).

## 결정

첫 패키지 런타임은 **Eclipse Temurin 21.0.12.1+1-LTS, macOS aarch64 HotSpot JRE**로 고정한다.
공식 JRE의 `Contents/Home` 전체를 사용하며 BXDL이 jlink로 모듈을 더 줄이지 않는다. `java --list-modules`의
실제 49개 모듈에 `jdk.management`가 포함된다. 공급 `release`의 MODULES는 빌드 JDK 모듈도 열거하므로
실제 런타임 모듈 목록으로 대신하지 않는다. 기존 Oracle21.0.7 jlink는 과거 시험 근거로 보존한다.

공식 [릴리즈](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.12.1%2B1)에서 공급하는
JRE·원본 소스·빌드 metadata·SBOM을 확인했다. Temurin의 전체 JRE 공급과 명시 소스/빌드 근거가 이번 선택 이유다.
Zulu의 Mac JRE도 대안이나 이번 단계에서 복수 런타임을 지원하지 않는다. Corretto21의 Mac 공식 목록은 JDK 중심이다.

정확한 binary/source/launcher 및 Home 259개 파일의 hash는
[선정 lock](../packaging/runtime-lock.temurin-21-macos-arm64.json)에 둔다. latest URL로 자동 갱신하지 않는다.
새 vendor/patch/파일 변경은 새 lock·제품 패키지·영향받는 인수를 거친다. 엔진 JAR는 RC1 그대로 유지한다.

## 준비 도구와 제품 UX

Rust `runtime stage`는 패키지 제작자용 오프라인 준비 명령이다. 추출한 JRE Home과 외부 신뢰 lock을 받아
파일 전체 경로·크기·SHA256·실행 권한을 확인한 후 새 폴더에 runtime과 provenance를 만든다.
JVM·다운로드·서비스 실행은 하지 않는다. archive 출처/서명 검증도 수행하지 않으며 결과에 이를 명시한다.
사용자는 설치 마법사에서 Java 배포판을 고르거나 시스템 Java를 설치할 필요가 없다.

기존 package/engine lock schema는 유지한다. full runtime inventory는 서명 package에 포함되어 설치·기동 때
재검증된다. runtime lock은 archive hash·대응 소스·259개 기대 파일을 고정하며 provenance 원문도 package에 포함한다.
추가/누락/변조·경로 탈출·외부/순환/디렉터리 링크·위험 mode를 거부한다. 안전한 내부 파일 링크는 독립 regular file로
복사하지만 실제 선정 archive에는 링크가 없었다. 기존 목적지를 덮어쓰지 않으며 실패한 새 부분 폴더는 보존한다.

Mac `.jre/Contents` 외곽의 Info.plist·CodeResources·MacOS/libjli.dylib 3개 파일은 Home 실행 layout에 포함하지 않는다.
제외 원본 hash는 snapshot에 보존한다. Home 파일 bytes는 유지하고 mode만 0644/0755로 정규화한다.
따라서 원래 Mac bundle 서명·notarization이 BXDL package까지 유효하다고 주장하지 않는다.

## 라이선스·출처

[공식 FAQ](https://adoptium.net/docs/faq/)와 포함 LICENSE는 GPLv2 + Classpath Exception을 명시한다.
원본 legal tree를 전부 보존한다. 대응 OpenJDK source archive와 고정 Temurin build scripts·metadata도 확보했다.
소스의 bundled native 라이브러리와 cacerts, build scripts의 certdata/mk-cacerts 존재를 확인했다.
고객 공급 시 binary와 함께 소스 자료를 동봉하거나 동등하게 접근 가능한 배포 경로에 제공한다.
현재는 로컬 개발 인수용 보존이며 upstream URL 기록만으로 BXDL의 소스 제공 완료를 주장하지 않는다.

공급자 SBOM의 JRE Component hash는 실제 공개 archive hash와 다르다. 원인은 미확정이다.
원본 SBOM은 바꾸지 않고 증거로 보존하며 실제 파일을 기준으로 runtime-only CycloneDX 명세를 별도 만든다.
NIGO/RocksDB/crypto native·Rust 전체 전이 의존성 SBOM/NOTICE 완성은 별도 출시 조건이다.

## 공동 검증과 남은 범위

- BXDL: vendor/build/bytes 선정, source/legal 보존, runtime staging, 실제 서명 패키지 설치·init·LaunchAgent·stop/restart.
- NIGO: 필수 모듈·옵션·native/tmp 조건 회신, 같은 RC1 JAR+선정 JRE의 엔진·crypto/mTLS/QBFT 검증.
- 공급자 현재 testEngineArtifact는 engineDistribution에 의존하여 JAR를 재빌드하고 child Java는 java.home에 묶인다.
  외부 JAR path/hash와 Java 실행 파일을 지정하고 JAR 전후 hash를 확인하는 독립 검증 경로 또는 동등한 근거를 요청한다.
  이미 공급한 RC1 bytes를 다시 만들지 않는다. 오류가 확인되면 main 수정·새 RC 규칙을 따른다.
- 소비자 단일 노드 시험과 공급자 4-validator/crypto 결과를 구분한다. 공급자 회신을 받기 전 동의/PASS로 표시하지 않는다.
- Mach-O launcher minOS11.0은 BXDL 지원 최소 OS가 아니다. 공급자 플랫폼 표와 실제 macOS26.6.2 시험을 구분한다.
  최소 OS matrix·오프라인 환경·4-validator·전체 G1-M·운영 출시 인수는 남아 있다.

## 공급자 회신 수신 — 2026-10-19

[NIGO PR #212의 J1~J3를 검토·수용했다](../contracts/nigo/jre-feedback-2026-10-19/README.md). 지정 RC1/Temurin의 공급자9사례 PASS이며 기존 lock을 유지한다. 원문 revision/hash와 검토 범위는 수신 원장을 따른다. 이후 BXDL 제품 인수와 전체 공급망 확인을 진행하며, 위 요청 당시의 대기 상태와 기존 시험 결과는 이력으로 보존한다.
