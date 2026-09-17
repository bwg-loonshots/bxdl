# BXDL 개발 계약 v1

- `cli-result.schema.json`: CLI 공통 JSON envelope.
- `package-spec.schema.json`: builder가 받는 expected identity metadata. inventory는 builder가 생성한다.
- `package-manifest.schema.json`: 서명되는 package inventory·제품·엔진·runtime 설명.
- `engine-lock.schema.json`: 명시적인 개발 후보 cold 실행의 외부 신뢰 JAR/Java hash 및 expected engine-info.
- `instance.schema.json`: 제품 JSON 설정. NIGO canonical schema가 아니다.

모두 development 단계의 BXDL 소비/표현 계약이다. JSON Schema는 구조를 표현하고 실행 코드는 경로·중복 key·정확한 inventory·hash·합계·권한 등 추가 조건을 검증한다. schema 파일만 검사한 결과를 runtime verifier 결과로 대신하지 않는다.

2026-09-17 Rust 전환에서 기존 Linux v1 구조와 CLI envelope를 유지하고 development `darwin/arm64` profile을 추가했다. platform의 oneOf는 Mac none/none과 Linux glibc/2.34 조합을 분리한다. version data의 Rust 구현·Mac 우선 식별은 추가 정보이며 실제 service 인수 결과는 NOT_CHECKED다.

NIGO의 PROPOSED 계약·개발 후보 manifest/fixture를 `contracts/nigo/development-2026-09-17/`에 원문 snapshot으로 수신했다. 정식 release 승인이나 모든 운영 API 인수 완료를 뜻하지 않는다. engine contractStatus는 `proposed`만 허용하고 향후 공급자 manifest와 mapping을 검증한 뒤 공식 소비 형식으로 확장한다.

`setup-draft.schema.json`은 중간 입력을 저장하는 BXDL 내부 초안 계약이다. 필수 입력이 모두 채워져도 설치·초기화·엔진 검사 완료를 뜻하지 않는다. 완성된 설정 출력은 기존 `instance.schema.json`을 따르며 참조 경로는 절대 경로로 고정한다. 초안 파일을 직접 편집하지 않고 `setup --resume`으로 수정한다.
