# RC.4 수신·패키징·관리 CLI 조합 인수 계획

- 작성: 2026-11-15. BXDL 기준 `241038cf5f594dfc603318be5a3aba4883990237`.
- 상태: **RC4-01 실제 수신·정적 identity·선정 JRE의 engine-info PASS. RC4-02 계약 영향·fresh 시험 자료 준비·회귀도 PASS. RC.4 lock NOT_SELECTED, 제품 인수 NOT_RUN. 다음은 RC4-03이다.** [수신 결과](../contracts/nigo/engine-rc4-2026-11-15/README.md), [RC4-02 결과](../results/2026-11-15-rc4-fixture-preparation.md), [계약·fixture 영향](./2026-11-15-rc4-contract-fixture-impact.md).
- 결정: 새 사용자 검증의 후보를 RC.4로 준비한다. 기본 순서는 RC.4 수신 → 새 runtime/관리 CLI 조합 인수 → CLI05-04 사람 과업이다. RC.3 기반 안내서 준비·기존 기능 검증은 병행할 수 있다.
- 현재 운용 지원은 기존 RC.3 그대로다. RC4-01은 별도 수신 기록을 추가했으며 lock/spec·지원 profile·설치된 실행 파일·서비스·DB를 변경하지 않았다.
- 여기의 RC.4는 NIGO 엔진 후보 `nigo-engine-v0.1.0-rc.4`이며, 이전 BXDL 로드맵의 R4 단계와 구분한다.

## 1. 확인한 공급 상태와 근거

기준은 [최종 인계 문서](https://github.com/bwg-loonshots/nigo-protocol/blob/7d15cd394ca9f75a8ab7eee35c0f8a7bae3b0977/requirements/REQ-0003-engine-rc4-handoff.md) revision `7d15cd394ca9f75a8ab7eee35c0f8a7bae3b0977`이다. 읽은 NIGO checkout HEAD는 `57539206`이며 미추적 리뷰 초안은 공급 근거에 포함하지 않았다. [누적 변경 검토](https://github.com/bwg-loonshots/nigo-protocol/blob/7d15cd394ca9f75a8ab7eee35c0f8a7bae3b0977/requirements/REQ-0003-rc3-cumulative-supply-review.md)의 11/12 미공급 상태와 추정 Console fingerprint는 당시 이력이다. 현재 공급 여부와 최종 pin은 11/15 인계를 따른다.

NIGO는 동일 고정 JAR의 G1~G6, 게시·새 다운로드 대조를 완료해 `DELIVERED`로 판정했다. BXDL은 11/15 계획 검토 때 GitHub API로 [prerelease](https://github.com/bwg-loonshots/nigo-protocol/releases/tag/nigo-engine-v0.1.0-rc.4)의 draft=false/prerelease=true, release/asset ID·크기·서버 digest와 annotated tag의 peeled source/tree를 대조했다. 이 원격 메타데이터 확인과 별도로, PR #21 머지 뒤 RC4-01에서 새 다운로드·로컬 checksum·전체 identity·실제 engine-info까지 검증했다. [소비자 수신 기록](../contracts/nigo/engine-rc4-2026-11-15/consumer-receipt.json)에 실제 수행 범위를 구분했다.

| 구분 | 상태 |
| --- | --- |
| NIGO 공급자 G1~G6 및 재다운로드 | 인계상 PASS / DELIVERED |
| BXDL의 인계·원격 release/tag 메타데이터 검토 | 완료 |
| BXDL archive 수신·전체 파일·실제 engine-info 대조 | RC4-01 PASS — 공급24파일·선정 JRE259파일 전후 불변, engine-info1회 exit0·stderr0 |
| RC.4 lock/spec 및 runtime package 선택 | NOT_SELECTED |
| RC.4 외부 관리 CLI 지원·Mac 제품 조합 인수 | NOT_IMPLEMENTED / NOT_RUN |
| CLI05-04 작성자가 아닌 사용자 과업 | NOT_RUN |

인계의 G1~G6는 NIGO 공급 gate다. BXDL의 G1-M·G1-L·G1-D/G2와 같은 완료 기준으로 세지 않는다. 원본 manifest의 `supplierTests=NOT_RECORDED`, `consumerAcceptance=NOT_RUN`, native `NOT_CHECKED`는 그대로 보존하고 공급자의 별도 evidence를 해당 digest에 연결한다. RC.3 결과를 새 후보 PASS로 복사하지 않는다.

## 2. 수신 시 대조할 고정 값

아래는 인계가 지정한 값과 재사용할 BXDL runtime lock의 Java launcher 값이다. RC4-01에서 실물과 대조했으며 실제 수신·실행 근거는 별도 소비자 기록에 남겼다.

| 항목 | 고정 값 |
| --- | --- |
| 태그 / release 라인 | `nigo-engine-v0.1.0-rc.4` / `release/engine-0.1` |
| 엔진 source / 계약 revision | `037c98c315d5322fa7da266ca0254d61f3863622`, dirty=false |
| source tree | `335859ac6ddbf4cb05cfa47c0b53a164bb36fabb` |
| annotated tag object | `2f0a0ca513e565ad59ecb660cc53961df5d11a32` |
| JAR 이름·크기 | `nigo-node-0.0.1-SNAPSHOT.jar`, 164,133,636 bytes |
| JAR SHA-256 | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` |
| 원본 manifest SHA-256 | `a3af07fcf1a933123ede138a6e5d27150469bce8040d0fc77340a38b86bb28c6` |
| 엔진 계약 fingerprint | `c604b1d24097015b2839d97f2bc9d45e3fff91ed6984665d1aa827a155c7b6e8` |
| 실제 Console fingerprint | `a7c768df5d1ad8a09060cbab33b6fd3f0e3475d4ee0f90d7c76c445433ab33b6` |
| archive 이름 | `nigo-engine-v0.1.0-rc.4-56887aa12f23.tar.gz` |
| archive 크기·SHA-256 | 157,700,874 bytes / `2c41581ff531e04a0302bab9cec268628929430858e1e8eba324ae1b75edd24e` |
| checksum asset 크기·SHA-256 | 110 bytes / `51d282cf1509b2595551a91363148045fa13a482236623ed21e6c2503ab3c591` |
| release / archive / checksum ID | `445809685` / `749821483` / `749821484` |
| 공급 inventory | 24파일: 23 payload + `SHA256SUMS` |
| JRE | 기존 선정 Temurin21.0.12.1+1-LTS, Java21, macOS arm64, full JRE259파일 |
| Java launcher SHA-256 (기존 BXDL runtime lock) | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |
| 공급자 JRE inventory 원문 SHA-256 | `fb779d849cdb9153b694b3ad368dc64b178af71a118d148dcd4d51a3e7853761` |

release의 서버 불변 보호는 `immutable=false`다. 인계에도 release branch `protected=false`가 기록돼 있으므로 이동 가능한 이름만으로 수신하지 않는다. full source·고정 digest·크기·전체 파일을 계속 대조한다. checksum 파일이나 서버 digest만으로 배포자 서명 인증이 완료됐다고 표시하지 않는다.

JRE는 [현재 runtime lock](../packaging/runtime-lock.temurin-21-macos-arm64.json)을 재사용 후보로 유지한다. RC.4 채택에 JRE 버전 변경을 섞지 않으며 실제 수신·조립 JRE inventory와 launcher bytes는 다시 대조한다. 공급 archive에는 JRE·BXDL CLI·고객 PKI 생성기·공식 source-free QBFT 생성기·새 developer kit가 포함된다는 보장이 없다.

## 3. RC.3 대비 제품 영향과 전환 원칙

| 변경 영역 | BXDL 계획에 반영할 내용 |
| --- | --- |
| KZG 장애 전파·BLS, 공개 EVM/내부 Native host 경계, Type-4 call/estimate | 공급자의 exact-JAR evidence를 연결하고 선정 package의 native load·노출 API 및 거래 통합에 필요한 검사를 정한다. canonical 전체 재빌드를 수신 절차로 만들지 않는다. |
| QBFT/BLOCK_SYNC·bounded live 복구, snapshot·historical proof·retention | fresh 동종 4-validator의 거래·catch-up·무정족수·live 복구·round-skew 재시작을 새 package로 인수한다. 변경된 관측 필드와 기존 parser를 대조한다. |
| RocksDB WAL·retention 행 차분 | 정상 close·같은 RC.4 DB 재시작·입력 보존을 검사한다. 공급자 crash 회귀를 BXDL power-cut/장기 운용 승인으로 승계하지 않는다. |
| 표준 Native artifact/catalog/pin·Console | 새 genesis·새 fixture를 사용하고 실제 Console/source/contract identity를 고정한다. 예전 Native catalog 입력과 시험 숫자를 복사하지 않는다. |

**RC.4는 fresh 개발망 전환으로 계획한다.** RC.3 data/control/key와 기존 archive·lock·인수 이력은 보존한다. 시험에는 새 DB·새 genesis와 분리된 시험 자격증명·포트를 사용하며 RC.3/RC.4 노드를 같은 망에 섞지 않는다. 사용자 기존 DB를 열어 교체 가능성을 시험하거나 자동 초기화·삭제하지 않는다.

RC.3 표준 catalog DB가 새 JAR에서 거절될 것이라는 NIGO 설명은 소스·artifact 대조에 따른 예상이다. 실제 구 DB migration 시험 PASS/FAIL로 기록하지 않는다. 엔진 계약 4파일 fingerprint가 같아도 genesis DB·wire·rolling 호환을 뜻하지 않는다. RC.4로 만든 DB를 RC.4로 다시 여는 재시작과 RC.3→RC.4 upgrade는 서로 다른 과업이다. 후자는 이번 계획에서 지원하지 않는다.

## 4. 현재 코드에서 바꿔야 할 지점

| 영역 | 현재 제약 | 예정 변경 |
| --- | --- | --- |
| `packaging/engine-lock.rc3-temurin-development.json`, `package-spec.rc3-temurin.example.json` | RC.3 전용 source/JAR/Console·engine-info | RC.3 파일 보존. 수신 후 RC.4 전용 snapshot·lock/spec을 추가하고 명시 선택 |
| `src/engine/instance/executor.rs` | `BXDL_RC3_EXECUTOR_V1`의 archive·manifest·실행기·JAR·Java exact pin | 후보별 인증 profile 추가. RC.3 유지, RC.4 package를 실제 조립한 뒤 그 pin만 등록 |
| `src/engine/instance/executor/reply.rs` | engine 응답의 source/JAR/Java도 RC.3 상수 대조 | 선택된 인증 profile의 expected identity로 검증. RC.3/RC.4 교차 응답·가짜/미지원 조합 거절 |
| `src/engine/setup_plan.rs`, `src/engine/instance/service.rs` | 미지원 package의 same-CLI 검사와 worker의 exact CLI 검사 | 새 지원 profile 연결과 실제 package 내부 worker 대조. 임의 실행기 허용이나 기존 검사 제거로 해결하지 않음 |
| `tests/acceptance/candidates.py`, `prepare_fixture.py`, `fixture_guard.py`, `run_acceptance.py` 및 회귀 | rc1~rc3 후보와 rc2/3 복구 분기, 기존 generator 호출 | 명시 rc4 후보·실제 fixture identity·복구 시나리오 추가, 기존 후보 동작 보존 |
| `run_management_service.py`, `run_management_acceptance.py` 및 회귀 | RC.3 archive/실행기/JAR·payload270파일/325,299,323bytes 고정 | 검증한 candidate/runtime profile별 입력 계약. RC.4의 실제 manifest에서 inventory 산출, RC.3 숫자 자동 승계 금지 |
| `src/cli.rs`, `src/management/probe.rs` | `executor.rc3-legacy-v1` capability와 probe 계약 | 기존 capability 보존, RC.4 지원 식별과 관리 CLI 업데이트 승인 기준을 함께 정의 |
| `tools/build-management-cli.py`, 지원 문서·결과 | 외부 CLI와 runtime이 독립 공급물 | 새 runtime에 대응하는 관리 CLI의 별도 source/binary/archive identity·지원 조합·업데이트/복귀 범위 기록 |

### 실행기와 외부 관리 CLI의 빌드 순서

새 package hash를 그 package 내부 실행기에 내장하려고 하면 자기 자신의 hash에 의존하는 순환이 생긴다. 다음 순서를 따른다.

1. 필요한 native adapter 변경과 `umask 077`이 포함된 **내부 실행기 A**를 clean source에서 빌드·고정한다.
2. A + 수신한 RC.4 JAR + 선정 JRE로 **runtime package R**을 조립·서명·검증하고 archive/manifest/실행기 identity를 고정한다.
3. R의 exact pin을 지원하는 **외부 관리 CLI B**를 별도 source revision에서 빌드한다. `BXDL_RC4_EXECUTOR_V1`은 예정 식별자이며 현재 구현된 capability가 아니다.
4. B가 R의 A에 위임하는 실제 경로와 A가 자기 bytes의 worker로 실행되는 경로를 검사한다. B를 R 안에 덮어쓰거나 profile 추가만으로 R을 재조립하지 않는다.

새 관리 CLI가 RC.3도 계속 지원하는지 회귀한다. RC.4에서 CLI update/rollback을 인수하려면 양쪽 관리 CLI가 **같은 RC.4 profile을 지원**해야 한다. RC.3 전용 구형 관리 CLI를 RC.4의 정상 rollback 대상으로 간주하거나 CLI rollback을 엔진/DB rollback으로 설명하지 않는다.

### 새 시험 자료 준비

현재 `prepare_fixture.py`는 고정 JAR 내 `QbftDevnetGenerator`를 호출한다. RC.4 고정 source에는 해당 helper가 있지만 공식 source-free 생성기 공급 계약과는 구분한다. RC4-02에서 준비 전용 후보에 RC.4 pin을 추가하고 `prepare_fixture.py`/`fixture_guard.py`의 준비 경로와 회귀를 구현했다. 실제 수신 JAR의 helper로 새 자격증명·chain/network/product·서명 거래를 준비하고 catalog lock을 정적으로 대조했다. 실행 후보 목록은 RC1~3를 유지하며 준비 성공을 runtime 지원으로 승계하지 않는다. genesis DB 생성은 RC4-05/06의 명시 init이다. 이 단계는 확정 runtime을 요구하는 RC4-04 실행·관리 인수 하네스와 구분한다. 이번 helper 사용은 PASS했으며 별도 NIGO 선행 구현 요청은 없다. 이후 사용할 수 없으면 같은 후보용 fixture/도구 지원을 요청한다. 후보 JAR를 main에서 재빌드하거나 RC.3 genesis·서명 거래를 새 망에 복사해 우회하지 않는다. 이 준비기는 고객 PKI 발급 기능으로 배포하지 않는다.

## 5. 단계별 실행 계획과 완료 기준

RC4-01/02는 DONE이며 RC4-03~08은 PLAN이다. PR 단위는 아래 단계의 의존 관계를 유지하면서 묶을 수 있으며, archive 수신 PASS·제품 PASS·사람 과업 PASS는 따로 기록한다.

| 순서 | 산출물·작업 | 선행·완료 기준 |
| --- | --- | --- |
| RC4-01 수신 검증 · DONE | 새 수신 경로, [2026-11-15 정제 수신 기록](../contracts/nigo/engine-rc4-2026-11-15/README.md) | 신뢰한 archive hash/크기 → 해제 전 24개 일반 파일·상대경로/중복/link/용량 검사 → 전체 checksum/JAR/원본 manifest/계약 → 선정 Java의 실제 engine-info·실행 전후 불변. lock 선택과 서비스 실행은 별도 |
| RC4-02 계약·fixture 영향 확정 · DONE | [영향 설계](./2026-11-15-rc4-contract-fixture-impact.md), [실제 fresh 준비·회귀 결과](../results/2026-11-15-rc4-fixture-preparation.md)와 유한 시험 목록 | RC4-01. `candidates.py`의 RC.4 pin·`prepare_fixture.py`/`fixture_guard.py`의 최소 지원과 해당 회귀를 먼저 구현. 새 후보로 만든 fresh genesis 입력·제품/native 입력·거래·시험 자격증명과 실제 helper 호출 PASS. genesis DB 생성·native 실행은 미수행. 공급자 evidence를 digest별로 연결하고 통합 검사 범위 확정 |
| RC4-03 runtime 조립·명시 선택 | RC.4 전용 engine lock/spec, 내부 실행기 A, signed development runtime R, inventory | RC4-01/02. 받은 JAR bytes 유지, JRE 전수 대조, clean 실행기/source 고정, 새 archive 전체 검증. RC.3 lock/설치본 무변경; 새 package/JRE/실행기 identity 기록 |
| RC4-04 외부 CLI·인수 도구 | exact RC.4 profile·reply 검증·runtime pin이 필요한 실행/관리 인수 하네스 완성, 별도 관리 CLI B | RC4-03의 고정 R. `make check`·Linux 타입 검사·profile 교차/변조/UNKNOWN 회귀와 기존 RC.3 호환 검사. 무조건 허용·임의 fallback 없음 |
| RC4-05 Mac 설치·서비스 | 같은 R+B의 순차 setup·apply·이름 기반 운용 결과 | RC4-04. 새 control/data에서 init/start/status/logs/diagnose/stop·같은 RC.4 DB 재시작, 취소/재개/원본 공급 경로 부재/조회 무쓰기, report0600·완전수집, worker/JRE/payload 불변·정상 정리 |
| RC4-06 새 동종망·변경 영향 | 같은 R+B의 4-validator mTLS·거래/복구·선정 통합 검사 | RC4-05. 4/4→3/4→catch-up, 2/4 무정족수, live 복구는 원 거래 무재제출, 같은 RC.4 DB round-skew 재시작은 명시 재제출과 구분, wrong-pin, 필요한 Type-4/KZG/Native/Console 정합. RC.4 지원 관리 CLI 두 버전의 실행 중 업데이트/복귀와 worker 불변, 정상 stop·PID/job/포트·inventory 독립 대조 |
| RC4-07 정식 소비자 회신 | 수신/선택/제품 결과·잔여를 NIGO REQ-0003에 회신 | RC4-01의 수신 사실은 먼저 공유 가능, RC4-05/06 결과는 완료 후 별도 추가. 외부 CLI/내부 실행기/runtime/JAR/JRE/source/harness·PASS/FAIL/NOT_RUN 분리. NIGO 수정은 전용 worktree/브랜치 |
| RC4-08 / CLI05-04 사람 과업 | 고정 배포본·안내서·기록 양식, 작성자가 아닌 사용자의 결과 | RC.4 기준 실제 수행은 RC4-05/06 뒤. 처음 설치·한 항목 수정·다음 날 재개·새 터미널 운용·실패 진단을 관찰. 안내/양식 준비는 병행, 자동 PTY 결과로 사람 과업을 완료 처리하지 않음 |

RC4-05의 report0600은 [기존 개선](./2026-10-23-private-initialization-report.md)을 새 내부 실행기/JVM 조합에서 확인하는 항목이다. 기존 RC.3의 0644 보고서 복구는 별도 과제로 유지한다. 설치 준비에서 가져오는 입력도 RC.4 fresh fixture여야 하며 기존 사용자 자료를 시험 대상으로 선택하지 않는다.

RC4-06의 공급자 G4/G5 성공은 시험 선정 근거로 재사용하지만 BXDL 서비스 adapter·설치 inventory·클라이언트 응답 parser의 제품 인수를 대체하지 않는다. 공급자의 438/1,634/751/69 등의 시험 수, 10.688/29.365초, child15개를 BXDL 결과나 SLA로 복사하지 않는다. 실제 실행별 source/JAR/harness·입력·횟수·시간·skip과 미실행을 기록한다.

## 6. 실행 환경·증거와 NIGO 의존성

RC4-01에서 다운로드·정적 대조와 선정 JRE의 실제 engine-info를 완료했다. 엔진 호출 전 검증기 정렬·빈 속성 파싱 오류2건은 이력을 보존하고 수정했다. JVM 호출은 총4회(속성2·module1·engine-info1), 자동 재시도0회다. 이 수신 단계에서는 노드·서비스·DB·다중 노드·generator를 실행하지 않았다. 후속 RC4-02는 준비 helper1·오프라인 Node 서명1·keytool4를 별도 새 경로에서 실행했고 노드·DB·서비스는 계속 미실행이다. 후속 실물 시험은 전용 장비 또는 업무와 분리한 Mac 시간에 새 경로·포트·최대 동시 노드 수·감독 시간·횟수·정리 기준을 정해 수행한다. 기본은 각 시나리오 1회, 자동 재시도 0이며 실패 원자료와 미실행 항목을 보존한다. 종료가 불명이면 상태를 확인한 뒤 정상 stop 절차로 정리하고 기존 데이터 삭제나 성공까지 재실행하지 않는다. 장기 soak/성능 MAIN을 자동으로 포함하지 않는다.

| 의존 사항 | 현재 판단 |
| --- | --- |
| 새 NIGO 엔진 기능·새 RC 공급 | 선행 대기 없음. RC4-01 수신·RC4-02 준비 PASS 뒤 RC4-03 runtime 조립·명시 선택이 다음 |
| 실제 archive 접근/무결성·engine-info 불일치 | RC4-01에서 일치 확인. 향후 불일치 시 후보 선택을 중지하고 고정 identity/근거로 공급자에게 문의 |
| 새 fixture/helper | 공급 JAR의 실제 helper 준비 PASS. 고객 PKI/공식 생성기 공급·제품 인수와 구분하며 추가 선행 요청 없음 |
| 계약이 같은 fingerprint인 경우 | source/Console/Native·wire·DB 차이 검토와 실제 제품 인수는 계속 필요 |
| 기존 DB 이전·rolling upgrade | RC.4 공급 범위 밖. 요구가 생기면 별도 NIGO 계약·지원 버전 쌍부터 정의 |
| A7 | 엔진 기능 공급 대기는 아님. 제품 stop/독점/백업/repair 승인/UNKNOWN/재시작은 별도 구현·인수 |
| 공급망/J5 | Native 부록 실수신·제품 NOTICE/SBOM·대응 소스·최소 OS·출처/조건 판단을 병행. RC.4 공급 PASS만으로 완료하지 않음 |

원문 log·trace·자격증명·signed transaction·DB/WAL·archive는 Git에 넣지 않는다. 정제 결과에 exact identity·판정·증거 digest·한계를 남긴다. source/binary 변경, 내부 실행기/JRE 변경, 외부 CLI-only 변경의 재인수 단위를 계속 구분한다.

## 7. 기존 계획에서 유지하는 잔여

RC.3의 CLI-01~04, CLI05-01/02/03 및 보고서 권한 개선 결과는 유효한 과거 조합 근거다. RC.4의 대조 기준으로 보존하고 완료 상태를 취소하거나 RC.4 결과로 이름만 바꾸지 않는다. CLI05-04 안내 준비나 RC.3 실제 UX 피드백을 먼저 수행할 수 있지만 RC.4로 결과를 자동 승계하지 않는다.

Mac 로그인/로그아웃·sleep/wake·중단 복구와 REM-01 안전 제거, A7, J5/정식 릴리스, 지정 버전 쌍 engine upgrade, Linux/systemd·Docker/Compose는 각각 별도 잔여다. A7/G2 전체를 RC.4 기본 설치·네트워크 재인수의 선행으로 만들지 않으며 해당 기능을 새 후보에서 사용하려는 경우 그 인수를 추가한다. H2는 공급자의 회귀 backend로서 BXDL 제품 지원에 추가하지 않는다.

채널 development, `officialRelease=false`, 계약 PROPOSED, REQ-0002/0003 OPEN, 자체 조건 UNDECIDED·고객 납품 NOT_GRANTED를 유지한다. 이 문서의 계획 반영이나 공급 메타데이터 확인은 정식 출시·고객 납품·데이터 repair 승인으로 사용하지 않는다.
