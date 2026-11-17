# RC4-03 — Mac 개발용 런타임 조립·명시 선택

**PASS — RC.4 전용 lock/spec을 선택하고 내부 실행기 A·수신 JAR·선정 JRE로 서명한 개발용 런타임 R을 조립했다. 외부 관리 CLI B와 설치·서비스·네트워크 인수는 아직 수행하지 않았다.**

[정제 JSON](./2026-11-15-rc4-runtime-assembly.json) · [도입 계획](../design/2026-11-15-rc4-adoption-plan.md) · [engine lock](../packaging/engine-lock.rc4-temurin-development.json) · [package spec](../packaging/package-spec.rc4-temurin.example.json).

## 변경과 빌드 순서

RC4-02를 `d38027a474f1e8d4fb7b1d38ea17972ba88d93af`로 커밋한 뒤, 해당 커밋의 별도 clean clone에서 내부 실행기 A를 빌드했다. `BXDL_REVISION`에 full commit을 지정하고 실제 `version --json`으로 확인했다. Cargo의 offline·locked release 빌드와 빌드 전후 source 청결도 검사가 통과했다. Rust 코드는 변경하지 않았으며 기존 `umask 077` 정책을 포함한다. 신규 초기화 보고서의 실제 권한은 RC4-05에서 확인한다.

새 RC.4 lock의 `expected` 전체를 RC4-01에서 받은 실제 engine-info와 대조했다. Spec은 NIGO source/JAR·계약 fingerprint·선정 Java를 고정하고, 조립용 사본에는 A의 source revision을 채웠다. 새 lock/spec은 A를 빌드한 커밋 이후의 별도 입력이며 JSON에 각각의 SHA-256을 남겼다.

A → R → B 순서에서 R의 archive hash를 A에 내장하지 않는다. 이번에 고정한 R을 인증하는 외부 관리 CLI B와 `BXDL_RC4_EXECUTOR_V1` profile은 RC4-04 작업이다. 기존 RC.3 profile·lock/spec·패키지·수신 기록을 유지하며 기존 설치본에는 접근하지 않았다. NIGO 저장소도 수정하지 않았다.

## 고정된 공급물

| 항목 | 값 |
| --- | --- |
| 내부 실행기 source / tree | `d38027a474f1e8d4fb7b1d38ea17972ba88d93af` / `cf97bc426e2ab997cb72b6e301aa8c057e0e214d` |
| 내부 실행기 SHA-256 / 크기 | `d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647` / 4,219,840 bytes |
| NIGO source | `037c98c315d5322fa7da266ca0254d61f3863622` |
| 수신 JAR SHA-256 / 크기 | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` / 164,133,636 bytes |
| JRE | Temurin 21.0.12.1+1-LTS, macOS arm64, 전체 259파일 |
| Java SHA-256 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| engine lock SHA-256 | `d07b35ee5a630ef459bb26a492a1511547e47e483c40120b7474b67a90e1326d` |
| 새 runtime archive SHA-256 / 크기 | `a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60` / 207,705,667 bytes |
| manifest SHA-256 | `fe21e898f5ace8be57ad838620f14da1c3e07b3ffcfbcb969fd63a67854a142e` |
| payload inventory | **269파일 / 326,373,420 bytes** |
| 서명 | 외부 Ed25519 개발 시험 key, `verified-external-ed25519` |

259개 runtime 파일 외에는 실행기·JAR·runtime provenance·NOTICE·runtime SBOM·원본 공급 SBOM·runtime 안내/metadata/관측/source-supply의 10개 파일을 명시적으로 포함했다. 실제 manifest에서 파일 수·크기·mode·hash를 계산했으며 RC.3의 270파일 수를 승계하지 않았다. 정제 JSON에 runtime lock hash와 나머지 10파일 inventory를 함께 남겼다. 개인키·archive·실행 파일은 Git에 넣지 않았다.

## 실제 검증

- 원본 JRE 전체 259파일의 membership·hash·size·mode를 선정 lock과 대조했다. `runtime stage`가 생성한 파일과 조립 전후 원본·stage를 다시 확인했다.
- stage에 들어간 A·JAR·Java와 RC.4 lock으로 `engine inspect`를 1회 수행했다. 전체 identity와 JAR/Java pin이 일치했고 exit 0, stderr 0이었다. JVM은 engine-info 경로로만 실행했다.
- 개발 시험 key를 stage 밖에서 생성하고 `package build` 1회, `package verify` 1회를 수행했다. 전체 269파일과 manifest·archive identity 및 서명이 일치했다.
- 별도 archive reader로 entry 순서·종류·membership·mode·size·SHA-256을 전수 대조하고, raw manifest에 대한 Ed25519 서명을 OpenSSL로 다시 검증했다.
- 별도 검토자가 완성된 archive와 원본 입력·빌드 기록을 다시 대조했다. 공개키만 사용한 추가 OpenSSL 서명 검증 1회도 통과했다. 이 검토는 조립·JVM 재실행이 아니며 아래 controller 호출 수와 별도로 기록했다.
- 조립 controller의 직접 호출은 BXDL 4회와 OpenSSL 3회, 합계 7회다. 모두 exit 0, timeout 없음, 소유 process group 종료 확인. engine-info JVM 1회는 이 직접 호출 수와 구분한다. Cargo 빌드 및 version 조회도 별도다.

초기 controller 사전 검사에서 Python `Path` 객체 순서와 상대경로 문자열 순서가 달라 비교가 중단됐다. 이때 259파일의 membership·내용·권한은 모두 같았고 stage 생성·child 실행은 없었다. 실패 기록과 controller 원본을 보존하고 정규화한 상대경로 순서로 수정한 뒤 새 출력 디렉터리에서 조립했다. 성공한 조립·engine-info는 각각 1회이며 자동 재시도는 0회다.

이번 변경은 lock/spec·문서·검증 기록이다. 제품 및 시험 코드는 RC4-02 커밋과 동일하므로 [기존 `make check`](./2026-11-15-rc4-fixture-preparation.md)의 Rust 592 PASS, Python 112 PASS·2 SKIP, fmt·Clippy PASS를 같은 코드의 근거로 재사용했다. 이번 단계에서 전체 검사를 다시 실행한 것으로 집계하지 않는다. 이번에 새로 수행한 것은 clean release 빌드와 위 실물 조립·identity·무결성·서명 검사다.

## 다음 단계와 한계

다음 **RC4-04**에서는 고정 R의 archive/manifest/A/JAR/Java identity를 지원하는 외부 관리 CLI profile·응답 검증·실행/관리 인수 하네스를 구현하고 별도 관리 CLI B를 빌드한다. 이후 RC4-05/06에서 새 fixture와 DB로 설치·init·LaunchAgent·네트워크를 인수한다. 현재 `run_acceptance.py`의 실행 후보는 RC1~3이며 RC.4 준비 지원과 별개다.

이번에는 설치·cold preflight·genesis DB·native 실행·서비스·네트워크를 시험하지 않았다. 기존 RC.3 DB 이전·혼합망·rolling upgrade도 지원하지 않는다. 서명은 로컬 개발 시험 인증이며 Apple code signing/notarization, 전체 제품 NOTICE/SBOM·대응 소스·최소 OS·고객 배포 승인을 대신하지 않는다. 채널 development, `officialRelease=false`, 계약 PROPOSED, REQ OPEN, 고객 납품 NOT_GRANTED를 유지한다. 추가 NIGO 선행 구현 요청은 없다.
