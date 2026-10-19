# Temurin21.0.12.1+1 macOS aarch64 JRE 공급 기록

2026-10-19 공식 [릴리즈](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.12.1%2B1)와
Adoptium API, 별도 checksum 파일을 대조한 BXDL runtime 후보다.
[선정 lock](../../../packaging/runtime-lock.temurin-21-macos-arm64.json)과
[설계](../../../design/2026-10-19-jre-selection.md)를 따른다.

| 파일 | 의미 |
| --- | --- |
| supplier-metadata.json | 공급 tar.gz.json 원본. OpenJDK source/build commit·환경·build 옵션 포함 |
| supplier-sbom.json | 공급 CycloneDX 원본. 최종 JRE archive hash와 불일치하며 제품 완전성 근거로 사용하지 않음 |
| observed-runtime.json | 실제 launcher/modules·Home inventory·제외 외곽 파일·SBOM 차이·OS 관측 |
| source-supply.json | 대응 소스·빌드 자료 identity, 로컬 보존과 고객 제공 잔여 구분 |

JRE tar.gz SHA256 `dec50fc6f9fcd4fe3ae8cabf5a5fa68f6afc48841f7698e468e9aa5d54beed84`,48,144,965bytes.
공식 체크섬/API와 내려받은 파일이 일치한다. 공급자의 detached signature 검증은 미실행이며 TLS/공식metadata
대조를 서명 검증으로 표현하지 않는다. release immutable=false이므로 고정 hash 불일치 시 거부한다.

supplier metadata SHA256 `896513a2a6a7666367d6603d2a7b70c392a9383ecf18f3f115d561a3f125d418`.
supplier SBOM SHA256 `967727266657f95fff3cf0e40fb189d4efb4bcfeb6d10ecfe04dc0ecf718f1a4`.
그 안의 JRE Component는 `badb44db8b63600bb6abf1eec8c9854f7b90c37210e7a22c25c77523bfdac663`를 기록한다.
원인은 확인되지 않았고 이 원본을 수정하지 않는다. 실제 runtime 파일은 별도 lock/BXDL runtime BOM과 대조한다.

원본은 GPLv2+Classpath Exception 및 포함 third-party terms를 따른다. [공식 FAQ](https://adoptium.net/docs/faq/).
Home legal tree는 패키지에 보존한다. 이 snapshot은 실제 바이너리나 전체 대응 소스의 대체물이 아니며
고객 공급 전 source-supply의 소스 제공 방식과 전체 제품 NOTICE/SBOM을 완성해야 한다.
