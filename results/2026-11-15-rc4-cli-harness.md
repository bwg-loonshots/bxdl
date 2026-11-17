# RC4-04 — 외부 관리 CLI·인수 도구 구현 및 CLI 배포 검증

**PASS — RC.4 고정 런타임 지원과 인수 도구를 구현하고, 독립 관리 CLI의 빌드·서명·CLI 전용 설치·전환 검증을 완료했다. RC.4 노드 설치·DB·서비스·네트워크·실제 RPC 인수는 NOT_RUN이다.**

[정제 JSON](./2026-11-15-rc4-cli-harness.json) · [구현 설계·선정 RPC 사례](../design/2026-11-15-rc4-cli-harness-implementation.md) · [도구 사용법](../tests/acceptance/README.md) · [관리 CLI 안내](../docs/management-cli.md).

## 구현과 커밋

요청한 RC4-03 변경을 `a7a3cf142191bf69d61f9ce48a61b29b687cbf87`로 커밋했다. 이어 RC4-04 구현·회귀를 완료했고, 서명 배포 도구의 clean source 조건을 충족하기 위해 `65fe83f0d569fcf2231474342fa0d60ca72555d5`로 고정했다. 전체 검사 당시의 변경 코드21파일 hash가 이 커밋과 같음을 확인했다. 시험 도중 source 변경은 없었다. 별도 clean clone에서 B를 빌드하고 실제 `version --json`의 revision을 대조했다.

- `BXDL_RC3_EXECUTOR_V1`을 유지하고 `BXDL_RC4_EXECUTOR_V1`을 추가했다. archive·manifest·내부 실행기·JAR·Java·source·전체 engine identity를 같은 profile에 연결하고 교차 응답·변조를 거절한다.
- 새 B는 CLI 설치·업데이트·복귀 후보에 RC.3/RC.4 capability를 모두 요구한다. 실행 전 인증 실패와 dispatch 후 UNKNOWN을 구분하며 자동 재시도하지 않는다. 구형 bootstrap의 과거 정책까지 소급 변경하지 않는다.
- 일반·관리 인수 도구에 후보별 고정 runtime 검증을 연결했다. RC.4는 기존 R을 읽어 사용하며 재조립·재서명 경로를 거절한다. 일반 A+R 실행과 외부 B+R 운용 인수를 구분한다.
- 새 서비스 입력 준비 도구와 provenance 검사를 추가했다. RC4-02의 체인·자격증명을 새 위치에 복사하며 새로운 DB나 키를 생성하지 않는다.
- 읽기 전용 RPC21사례/52조회와 실패 부분 증거 보존을 구현했다. 실제 RPC는 아직 실행하지 않았다. 정상 signed Type-4·성공 authorization 격리·KZG fault·Native recipe 전체를 검사했다고 표시하지 않는다.

## 고정된 관리 CLI B

| 항목 | 값 |
| --- | --- |
| source / tree | `65fe83f0d569fcf2231474342fa0d60ca72555d5` / `547bf0be5c4a346fb1d08f540c9ece33405d9b39` |
| CLI SHA-256 / 크기 | `f08578809744d21313aad3c8cb39644107af67d5558661d2ecc9d81f2edb138c` / 4,219,952 bytes |
| CLI bundle SHA-256 / 크기 | `0b03f7324c6b2b735aed9ab7793e65d151af1445f93e0506068289d19cb03a23` / 1,911,951 bytes |
| CLI manifest SHA-256 | `8f5fb8301f043396087bb09023c3b48ccecc8d650cec7616cd9f54b76b6c4d29` |
| CLI payload | 실행 파일·zsh completion·안내 문서2개, 합계4파일 / 4,248,429 bytes |
| 서명 | 별도 외부 Ed25519 개발 시험 key, `verified-external-ed25519` |
| 그대로 사용하는 runtime R | `a9f67988e785260b35504bd79a19df4a4820e72eafbf537328ca2e31f7e16f60` |
| 그대로 사용하는 내부 실행기 A | source `d38027a474f1e8d4fb7b1d38ea17972ba88d93af`, SHA `d2d5c586bf2ab59ac3198cbcc0f89101fc18d31e6f4ec7812b428e481a63f647` |

R과 A는 조립하거나 덮어쓰지 않았으며 실행 전후 hash가 같았다. 관리 CLI와 runtime의 서명 key도 별개다. 개인키·archive·바이너리·서비스 입력 원문은 Git에 넣지 않는다.

## 검증 결과

`make check` 전체 PASS: **Rust596, Python153개 중151 PASS·2 SKIP, fmt·Clippy PASS**. `make check-linux`의 x86_64 Linux 타입 검사도 PASS다. 실제 Linux 실행은 아니다. Python의 두 skip은 기존 실제 CLI 배포 쌍·실제 바이너리 ZLE용 opt-in 검사이며, 이번 별도 CLI 검증으로 그 테스트를 실행 처리하지 않는다.

새 private CLI 전용 경로 두 개에서 다음을 실제 확인했다. JVM·노드·서비스 호출은 없었다.

| 실제 사례 | 결과 |
| --- | --- |
| B 서명 bundle 제작·검증 및 별도 archive inventory 대조 | PASS, clean revision과4파일 hash/size/mode 일치 |
| 새 B 설치·고정 진입점 version/info | PASS, RC.3/RC.4 capability와 새 revision 확인 |
| B → RC.3 전용 구형 CLI update 시도 | `CLI_CANDIDATE_FAILED` / exit3, 현재·이전 선택과 current link 보존 |
| 별도 구형 bootstrap 설치 → B update | PASS, 기존 관리 CLI에서 B로 업데이트 가능 |
| 위 업데이트 뒤 B → RC.3 전용 이전 CLI rollback 시도 | `CLI_CANDIDATE_FAILED` / exit3, B 선택 보존 |

직접 controller 호출은19개이며 두 capability 거절의 exit3은 기대 결과다. 나머지는 exit0, 자동 재시도0, 소유 process group은 모두 종료됐다. build 도구 내부 Cargo·version/bundle 호출은 이 직접 호출 수와 구분한다. 두 RC.4 지원 CLI 사이의 실행 중 update/rollback은 RC4-06에서 검증하며 이번에는 한 RC.4 지원 B만 빌드했다.

서비스 입력 준비 도구도 실제 RC4-02 자료로 실행했다. 새9개 입력과3개 공급 파일, 그 안의6개 자격증명을 확인했고 원본 chain의4-validator membership과 credential bytes를 유지했다. 실제 준비 당시에는 구현이 미커밋이어서 base `a7a3cf1`과 파일별 hash를 기록했고, 뒤에 `65fe83f`의 동일 코드임을 대조했다. child process0, 새 DB·키·노드 생성0이며 source 불변·새 data 부재·read-only provenance 재검사가 통과했다. 수동 입력의 실제 cold 검증과 `MANUAL_START_INITIALIZED` 도달은 RC4-05다.

별도 정적 검토에서 B bundle·두 설치본·시험 코드와 로그·서비스 입력 준비 근거를 대조했다. 공개키만 사용한 추가 OpenSSL 서명 검증1회도 통과했으며 위 controller19회와 별도다. 개인키·자격증명 본문은 재열람하지 않았고, 자격증명은 준비 시 계산한 hash와 현재 파일 종류·0600 권한까지 확인했다. CLI/JVM·빌드·시험은 다시 실행하지 않았다.

## 다음 단계와 한계

다음 **RC4-05**는 고정 R+B로 순차 설치·apply·명시 init·LaunchAgent start/status/logs/diagnose/stop, 보고서0600과 완전 수집, 같은 RC.4 DB 재시작을 검증한다. 실제 B→A 위임·A worker 경로는 이 제품 인수에서 확인한다. RC4-06에는 동종4-node·복구·선정 RPC와 RC.4 지원 CLI 두 버전의 실행 중 전환이 남아 있다.

RC.3 고정 profile과 기존 회귀는 유지했지만 RC.3 실제 네트워크를 이번에 다시 실행하지 않았다. 기존 DB migration·혼합망 rolling upgrade·정식 릴리스·고객 납품 승인도 범위 밖이다. 채널 development, `officialRelease=false`, 계약 PROPOSED와 REQ OPEN을 유지한다. 추가 NIGO 선행 구현 요청은 없다.
