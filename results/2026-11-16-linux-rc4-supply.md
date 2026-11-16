# LNX-02 Linux JRE·RC.4 공급물 고정과 도구 준비

2026-11-16 · **SUPPLY_PINNED / OFFLINE_VERIFIED / 실제 Linux 실행 NOT_RUN**

LNX-01은 `9a8593638c7cb66f4793b9ae831b515a6060957d`로 커밋했다. 선행 RC4-06 결과는 `9f460e6`에 보존했다. 이번 LNX-02는 사용자의 후속 답변에 따라 수신·도구 준비까지 수행했다. 실제 Linux 접속 정보는 필요하지 않으며 테스터가 직접 도구를 실행하고 정제 JSON만 반환할 수 있다.

## 선정·고정

- [고정 공급 기록](../contracts/runtime/temurin-21.0.13-linux-amd64/README.md): Temurin21.0.13+9-LTS Linux x64/glibc full JRE. 이날 공식 Java21 LTS 패치를 선정했으며 Mac21.0.12.1+1 조합과 구분한다.
- [runtime lock](../packaging/runtime-lock.temurin-21-linux-amd64.json):257파일·166,678,666 bytes. archive52,181,297 bytes·SHA `a535a1df9c6284d9618872984e026ae0352d1f7e9e0605425c0e6114f47351dd`.
- [Linux engine lock](../packaging/engine-lock.rc4-temurin-linux-development.json): 기존 RC.4 JAR SHA `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a`/164,133,636bytes와 expected identity 전체 유지. Linux Java SHA `7afc28801f744050aeba646b20e7314265279f76f9d0273b21a2b66411e1bf6d`만 새로 결합했다.
- [조합 identity](../packaging/linux-rc4-candidate.json): 공급물 pin이다. Linux runtime archive·A-L/C-L binary나 승인 실행기 profile을 만들었다는 뜻이 아니다.

## 실제 수행한 확인

공식 release의11개 공급 파일은 API size/digest와 일치하며 JRE·대응 source의 별도 checksum도 일치한다. source archive와 build 자료를 로컬 보존하고 고지9파일을 실제 읽었다. 공급 SBOM의 JRE component hash도 이 Linux archive와 일치한다. 원본과 [수신 기록](../contracts/runtime/temurin-21.0.13-linux-amd64/received-supply.json)을 보존하며 서명 검증·전체 고지/source 전달 완료로 확대하지 않는다.

Python [준비 도구](../tools/prepare-linux-runtime.py)는320개 tar entry의 전체 구조를 검증한 뒤145개 내부 legal symlink를 정규파일로 펼쳤다. 입력 bytes·전체 출력 inventory를 재검사했다. 별도 clean LNX-01 checkout의 Mac CLI를 release build하고 그 `runtime stage`로257파일 전체를 다시 대조했다. 이 단계의 symlinksMaterialized=0은 Python에서 이미 정규화한 입력을 받았기 때문이며 원 공급 링크145개와 모순되지 않는다. 두 도구 모두 Linux JVM을 실행하지 않았다.

고정 RC.4 JAR bytes도 다시 hash/size 대조했다. [정적 nested ELF resource 목록](./2026-11-16-linux-rc4-native-resources.json)은 x86_64 `.so` 존재만 기록한다. 다른 OS·musl resource도 포함될 수 있으며 presence는 Linux load/native/RocksDB PASS가 아니다.

## 검사와 전달물

**`make check` PASS: Rust623 PASS, Python207개 중205 PASS·2기존 opt-in SKIP, fmt·Clippy PASS.** Rust 구현은 LNX-01에서 변경되지 않아 Linux target 타입 검사는 재실행하지 않았다. Mac의 native probe 직접 실행은 예상한 `NATIVE_LINUX_X64_REQUIRED`/exit3으로 거절했고 출력 폴더를 만들지 않았다. 실행 안내의 Bash7개 블록도 구문 검사만 통과했다. JSON Schema로 Linux runtime/engine lock을 확인했고, 서로 독립적으로 작성한257파일 inventory가 일치했다. 새 테스트는 tar 변조·경로/링크·권한·부분 출력, native host·전체 pin·모듈 의미·종료 정리·입력 불변과 공개 결과의 범위를 검사한다.

[Linux 검증 안내](../docs/linux-validation.md)는 고정 소스의 native CLI 빌드, JRE 준비, CLI/Java/engine-info cold 실행과 결과 반환 절차를 제공한다. 이번 전달물은 개발용 기술 검증 자료이며 최종 사용자 설치 패키지가 아니다. Cargo 의존성이 없으면 빌드 중 네트워크가 필요하다. `result.json`만 공유하고 원문 `evidence/`와 작업 폴더는 테스터 장비에 보관한다.

로컬 전달 archive는 `dist/bxdl-linux-rc4-validation-kit-20261116.tar.gz`이며 Git에는 넣지 않았다. **323,996,959 bytes**, SHA-256 `eecbaf5c513b38e80e2d9114f5f931460da744aca399bd1ff14b95700d46cbc9`다. 압축 파일을 다시 읽어28개 구성 파일을 `SHA256SUMS`와 대조했고, 공급 JRE/JAR·실행 도구·lock이 선택한 bytes와 일치함을 확인했다. UID/GID·작성자 이름·시각 metadata는 정규화했다. 이 checksum과 후보 pin은 전달물 밖의 검토한 경로에서도 확인해야 한다.

검토 중 공급 `release MODULES`를 실제 JRE 목록으로 취급하던 부분, runtime 경로/소유자 검사와 query process-group 종료 경쟁의 원자료 보존을 보완했다. source build JDK69개 선언과 실제 JRE module 관측을 구분하고, root/호출 UID를 신뢰 범위로 한 경로 검증 및 정리 실패 보존을 회귀에 포함했다. 실제 Linux 실행 결과를 만든 것은 아니다.

## 남은 범위

LNX-02 전체 완료는 아니다. 실제 Linux CLI build·JRE/engine-info·native/RocksDB는 NOT_RUN이다. cold probe PASS만으로 native/JNI·DB 초기화·서비스·운영 package 인수를 인정하지 않는다. Linux R-L/A-L/C-L은 실제 build/조립 후 따로 고정한다. NIGO의 기존 Mac 공급 시험이나 RC.4 Mac PASS를 Linux 검증으로 승계하지 않는다. 현재 NIGO checkout은 수정하지 않았다.

실제 실행은 테스터의 JSON 회수 또는 선택적인 직접 SSH로 이어갈 수 있다. [Linux 우선 계획](../design/2026-11-16-linux-first-implementation-plan.md)에 따라 native/RocksDB와 systemd·단일노드 인수 후 Linux 사용자 UX 과업을 진행한다. WSL2는 후속이다. 상세 수치·hash·PASS/NOT_RUN은 [정제 JSON](./2026-11-16-linux-rc4-supply.json)을 따른다.
